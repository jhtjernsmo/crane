//! Containers in the app: Crane's own engine (crates/crane-box), one pod per
//! running container for now. Ports are only opened on 127.0.0.1.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

#[cfg(target_os = "linux")]
use crane_box::engine::{ContainerOptions, Engine, Forward, Pod, forward};

/// How long a container gets to stop before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(10);
const LOG_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortMap {
    pub host: u16,
    pub container: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    pub name: String,
    pub image: String,
    pub ports: Vec<PortMap>,
    pub env: Vec<(String, String)>,
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerView {
    pub name: String,
    pub image: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub ports: Vec<PortMap>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageView {
    pub name: String,
    pub size: u64,
    pub exposed_ports: Vec<u16>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxStatus {
    /// False where the engine cannot run (not Linux, or no user namespaces).
    pub available: bool,
    pub problem: Option<String>,
    pub images: Vec<ImageView>,
    pub containers: Vec<ContainerView>,
}

#[cfg(target_os = "linux")]
struct Running {
    image: String,
    pod: Arc<Pod>,
    ports: Vec<PortMap>,
    _forwards: Vec<Forward>,
}

pub struct Boxes {
    #[cfg(target_os = "linux")]
    engine: Option<Engine>,
    #[cfg(target_os = "linux")]
    running: Mutex<HashMap<String, Running>>,
    problem: Option<String>,
}

impl Boxes {
    pub fn new(base: std::path::PathBuf) -> Self {
        #[cfg(target_os = "linux")]
        {
            match crane_box::engine::Helper::this_program() {
                Ok(helper) => Self { engine: Some(Engine::new(base, helper)), running: Mutex::default(), problem: None },
                Err(e) => Self { engine: None, running: Mutex::default(), problem: Some(e.to_string()) },
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = base;
            Self { problem: Some("Containers run on Linux for now. Mac and Windows need a small virtual machine, which comes later.".into()) }
        }
    }

    #[cfg(target_os = "linux")]
    fn engine(&self) -> Result<&Engine, String> {
        self.engine.as_ref().ok_or_else(|| self.problem.clone().unwrap_or_else(|| "Containers are not available.".into()))
    }
}

#[tauri::command]
pub async fn box_status(boxes: State<'_, Boxes>) -> Result<BoxStatus, String> {
    #[cfg(target_os = "linux")]
    {
        let Ok(engine) = boxes.engine() else {
            return Ok(BoxStatus { available: false, problem: boxes.problem.clone(), images: vec![], containers: vec![] });
        };
        let images = engine.images().into_iter().map(|r| ImageView { name: r.name, size: r.size, exposed_ports: r.config.exposed_ports }).collect();
        let running = boxes.running.lock().await;
        let mut containers = Vec::new();
        for (name, r) in running.iter() {
            let state = r.pod.status().await.ok().and_then(|s| s.into_iter().find(|c| &c.name == name));
            containers.push(ContainerView {
                name: name.clone(),
                image: r.image.clone(),
                running: state.as_ref().is_some_and(|s| s.running),
                exit_code: state.and_then(|s| s.exit_code),
                ports: r.ports.clone(),
            });
        }
        containers.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(BoxStatus { available: true, problem: None, images, containers })
    }
    #[cfg(not(target_os = "linux"))]
    Ok(BoxStatus { available: false, problem: boxes.problem.clone(), images: vec![], containers: vec![] })
}

/// Pulls an image; progress goes out as `box-pull` events.
#[tauri::command]
pub async fn box_pull(app: AppHandle, boxes: State<'_, Boxes>, name: String) -> Result<ImageView, String> {
    #[cfg(target_os = "linux")]
    {
        let engine = boxes.engine()?;
        let record = engine
            .pull(&name, |p| {
                let _ = app.emit("box-pull", &p);
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(ImageView { name: record.name, size: record.size, exposed_ports: record.config.exposed_ports })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, name);
        Err(boxes.problem.clone().unwrap_or_default())
    }
}

#[tauri::command]
pub async fn box_remove_image(boxes: State<'_, Boxes>, name: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let wanted = crane_box::image_ref::ImageRef::parse(&name).map(|r| r.to_string()).unwrap_or(name.clone());
        if boxes.running.lock().await.values().any(|r| r.image == wanted) {
            return Err(format!("{wanted} is used by a container. Remove the container first."));
        }
        boxes.engine()?.remove_image(&name).await.map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        Err(boxes.problem.clone().unwrap_or_default())
    }
}

/// Starts a container in a pod of its own and opens its ports on 127.0.0.1.
#[tauri::command]
pub async fn box_run(boxes: State<'_, Boxes>, req: RunRequest) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let engine = boxes.engine()?;
        let mut running = boxes.running.lock().await;
        if running.contains_key(&req.name) {
            return Err(format!("There is already a container called {}. Remove it first.", req.name));
        }
        let image = engine.image(&req.image).ok_or_else(|| format!("{} is not pulled yet.", req.image))?;
        let pod = Arc::new(engine.start_pod(&req.name).await.map_err(|e| e.to_string())?);
        let opts = ContainerOptions { args: req.args.clone(), env: req.env.clone(), ..Default::default() };
        let spec = engine.prepare(&req.name, &req.name, &image, &opts).await.map_err(|e| e.to_string())?;
        pod.run(spec).await.map_err(|e| e.to_string())?;
        let mut forwards = Vec::new();
        for p in &req.ports {
            forwards.push(forward(Arc::clone(&pod), p.host, p.container).await.map_err(|e| e.to_string())?);
        }
        running.insert(req.name.clone(), Running { image: image.name, pod, ports: req.ports, _forwards: forwards });
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = req;
        Err(boxes.problem.clone().unwrap_or_default())
    }
}

#[tauri::command]
pub async fn box_stop(boxes: State<'_, Boxes>, name: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let running = boxes.running.lock().await;
        let r = running.get(&name).ok_or_else(|| format!("There is no container called {name}."))?;
        r.pod.stop(&name, STOP_GRACE).await.map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        Err(boxes.problem.clone().unwrap_or_default())
    }
}

/// Stops the container if needed and deletes its files (its log is kept).
#[tauri::command]
pub async fn box_remove(boxes: State<'_, Boxes>, name: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let r = boxes.running.lock().await.remove(&name).ok_or_else(|| format!("There is no container called {name}."))?;
        r.pod.shutdown().await.map_err(|e| e.to_string())?;
        drop(r);
        boxes.engine()?.clean_pod(&name).await.map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        Err(boxes.problem.clone().unwrap_or_default())
    }
}

#[tauri::command]
pub async fn box_logs(boxes: State<'_, Boxes>, name: String) -> Result<String, String> {
    #[cfg(target_os = "linux")]
    {
        Ok(boxes.engine()?.log_tail(&name, &name, LOG_BYTES))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        Err(boxes.problem.clone().unwrap_or_default())
    }
}
