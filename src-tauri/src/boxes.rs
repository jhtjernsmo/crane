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
#[cfg(target_os = "linux")]
use crane_box::stack::{self, StackFile, run::RunningStack};

use crate::AppState;

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
    /// The stack it belongs to; those are started and stopped together.
    pub stack: Option<String>,
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
    #[cfg(target_os = "linux")]
    stack: Mutex<Option<RunningStack>>,
    problem: Option<String>,
}

impl Boxes {
    pub fn new(base: std::path::PathBuf, policy: Arc<crane_core::policy::Policy>) -> Self {
        #[cfg(target_os = "linux")]
        {
            // Turned off by policy: no engine at all, so no command can start one.
            if !policy.containers {
                return Self { engine: None, running: Mutex::default(), stack: Mutex::default(), problem: Some(policy.refusal("Running containers")) };
            }
            match crane_box::engine::Helper::this_program() {
                Ok(helper) => Self { engine: Some(Engine::new(base, helper)), running: Mutex::default(), stack: Mutex::default(), problem: None },
                Err(e) => Self { engine: None, running: Mutex::default(), stack: Mutex::default(), problem: Some(e.to_string()) },
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (base, policy);
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
                stack: None,
                image: r.image.clone(),
                running: state.as_ref().is_some_and(|s| s.running),
                exit_code: state.and_then(|s| s.exit_code),
                ports: r.ports.clone(),
            });
        }
        drop(running);
        if let Some(st) = boxes.stack.lock().await.as_ref() {
            let states = st.pod.status().await.unwrap_or_default();
            for svc in &st.services {
                let state = states.iter().find(|c| c.name == svc.name);
                containers.push(ContainerView {
                    name: svc.name.clone(),
                    stack: Some(st.name.clone()),
                    image: svc.image.clone(),
                    running: state.is_some_and(|s| s.running),
                    exit_code: state.and_then(|s| s.exit_code),
                    ports: svc.ports.iter().map(|p| PortMap { host: p.host, container: p.container }).collect(),
                });
            }
        }
        containers.sort_by(|a, b| (a.stack.is_none(), &a.name).cmp(&(b.stack.is_none(), &b.name)));
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
        let in_stack = boxes.stack.lock().await.as_ref().is_some_and(|st| st.services.iter().any(|s| s.name == req.name) || st.name == req.name);
        if running.contains_key(&req.name) || in_stack {
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
        if let Some(st) = boxes.stack.lock().await.as_ref()
            && st.services.iter().any(|s| s.name == name)
        {
            return st.pod.stop(&name, STOP_GRACE).await.map_err(|e| e.to_string());
        }
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
        if boxes.stack.lock().await.as_ref().is_some_and(|st| st.services.iter().any(|s| s.name == name)) {
            return Err(format!("{name} belongs to the stack. Stop the stack to remove it."));
        }
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
        let pod = match boxes.stack.lock().await.as_ref() {
            Some(st) if st.services.iter().any(|s| s.name == name) => st.name.clone(),
            _ => name.clone(),
        };
        Ok(boxes.engine()?.log_tail(&pod, &name, LOG_BYTES))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = name;
        Err(boxes.problem.clone().unwrap_or_default())
    }
}

// ---- Stacks: crane.toml in the open collection ---------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackService {
    pub name: String,
    pub image: String,
    pub ports: Vec<String>,
    pub volumes: Vec<String>,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackInfo {
    /// False when no folder is open or it has no crane.toml.
    pub exists: bool,
    pub name: String,
    pub services: Vec<StackService>,
    /// What is wrong with the file, if anything.
    pub problem: Option<String>,
    pub running: bool,
}

#[cfg(target_os = "linux")]
fn read_stack(state: &AppState) -> Result<Option<(std::path::PathBuf, StackFile)>, String> {
    let Ok(col) = state.collection() else { return Ok(None) };
    let path = col.root().join(stack::FILE);
    let Ok(text) = std::fs::read_to_string(&path) else { return Ok(None) };
    Ok(Some((col.root().to_path_buf(), StackFile::parse(&text)?)))
}

#[tauri::command]
pub async fn stack_info(state: State<'_, AppState>, boxes: State<'_, Boxes>) -> Result<StackInfo, String> {
    #[cfg(target_os = "linux")]
    {
        let running = boxes.stack.lock().await.is_some();
        let empty = |problem: Option<String>| StackInfo { exists: problem.is_some(), name: String::new(), services: vec![], problem, running };
        let (folder, file) = match read_stack(&state) {
            Ok(Some(found)) => found,
            Ok(None) => return Ok(empty(None)),
            Err(e) => return Ok(empty(Some(e))),
        };
        Ok(StackInfo {
            exists: true,
            name: file.pod_name(&folder),
            problem: file.validate(&folder).err(),
            services: file
                .services
                .iter()
                .map(|s| StackService {
                    name: s.name.clone(),
                    image: s.image.clone(),
                    ports: s.ports.clone(),
                    volumes: s.volumes.clone(),
                    depends_on: s.depends_on.clone(),
                })
                .collect(),
            running,
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (state, boxes);
        Ok(StackInfo { exists: false, name: String::new(), services: vec![], problem: None, running: false })
    }
}

/// Writes a commented crane.toml to start from, if there is none.
#[tauri::command]
pub async fn stack_create(state: State<'_, AppState>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let col = state.collection()?;
        let path = col.root().join(stack::FILE);
        if path.exists() {
            return Err("This folder already has a crane.toml.".into());
        }
        std::fs::write(path, stack::TEMPLATE).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = state;
        Err("Containers run on Linux for now.".into())
    }
}

/// Starts the open collection's stack. `{{variables}}` in env values come
/// from the given environment, secrets from the keyring.
#[tauri::command]
pub async fn stack_up(app: AppHandle, state: State<'_, AppState>, boxes: State<'_, Boxes>, env_id: Option<String>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let engine = boxes.engine()?;
        let mut slot = boxes.stack.lock().await;
        if slot.is_some() {
            return Err("The stack is already running.".into());
        }
        let (folder, file) = read_stack(&state)?.ok_or("Open a folder with a crane.toml first.")?;
        let values = env_values(&state, env_id.as_deref(), &file).await?;
        let env_name = env_id.as_deref().and_then(|id| state.envs().get(id).map(|e| e.name.clone()));
        let fill = move |text: &str| {
            crane_core::vars::substitute(text, &|n: &str| values.get(n).cloned()).map_err(|missing| match &env_name {
                Some(env) => format!("{env} has no {}.", missing.iter().map(|m| format!("{{{{{m}}}}}")).collect::<Vec<_>>().join(" or ")),
                None => "crane.toml uses {{variables}}; pick an environment first.".into(),
            })
        };
        let running = stack::run::up(engine, &file, &folder, &fill, &|ev| {
            let _ = app.emit("stack-progress", &ev);
        })
        .await
        .map_err(|e| e.to_string())?;
        *slot = Some(running);
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, state, boxes, env_id);
        Err("Containers run on Linux for now.".into())
    }
}

/// Every variable the stack's env values use, read up front (secrets
/// from the keyring) so filling them in needs no more keyring calls.
#[cfg(target_os = "linux")]
async fn env_values(state: &AppState, env_id: Option<&str>, file: &StackFile) -> Result<HashMap<String, String>, String> {
    let names: Vec<String> = file.services.iter().flat_map(|s| s.env.values()).flat_map(|v| crane_core::vars::find_vars(v)).collect();
    let Some(env) = env_id.and_then(|id| state.envs().get(id).cloned()) else { return Ok(HashMap::new()) };
    let store = Arc::clone(&state.secrets);
    tokio::task::spawn_blocking(move || {
        let mut out = HashMap::new();
        for n in names {
            let Some(var) = env.vars.iter().find(|v| v.name == n) else { continue };
            if var.secret {
                if let Some(v) = store.get(&env.id, &n).map_err(|e| format!("Could not read the system keyring: {e}"))? {
                    out.insert(n, v);
                }
            } else {
                out.insert(n, var.value.clone());
            }
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn stack_down(boxes: State<'_, Boxes>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let engine = boxes.engine()?;
        let Some(st) = boxes.stack.lock().await.take() else { return Ok(()) };
        stack::run::down(engine, st).await.map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = boxes;
        Ok(())
    }
}
