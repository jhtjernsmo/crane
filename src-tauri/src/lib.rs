//! Tauri shell: opens the window and lets the UI send requests through Rust.
//! Requests go out from here, not from the webview, so there is no CORS and
//! local and internal addresses work like they do from curl.
//!
//! Secret values only ever travel from the UI to the keyring (`set_secret`),
//! never back: the UI learns only which secrets have a value.

mod secrets;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crane_core::collection::{Collection, Node, SaveResult};
use crane_core::env::{EnvFile, Environment, SecretStore, resolve};
use crane_core::http_file::{ReadRequest, SavedRequest};
use crane_core::vars::mask;
use crane_core::{RequestSpec, Response, mask_response};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

/// How long a request may take before Crane gives up on it.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const ENV_FILE: &str = "environments.json";
const SETTINGS_FILE: &str = "settings.json";
/// Methods that change data, so they need a yes first in a production environment.
const WRITE_METHODS: [&str; 4] = ["POST", "PUT", "PATCH", "DELETE"];

pub struct AppState {
    /// One client for the whole app, so connections are reused between sends.
    client: reqwest::Client,
    envs: Mutex<EnvFile>,
    env_path: PathBuf,
    secrets: Arc<dyn SecretStore>,
    /// The open folder of `.http` files. File commands only reach inside it.
    collection: Mutex<Option<Collection>>,
    settings_path: PathBuf,
}

impl AppState {
    fn envs(&self) -> EnvFile {
        self.envs.lock().expect("environments lock").clone()
    }

    fn collection(&self) -> Result<Collection, String> {
        self.collection.lock().expect("collection lock").clone().ok_or_else(|| "Open a folder first.".to_string())
    }
}

/// Small things to remember between runs. Never secrets.
#[derive(Default, Deserialize, Serialize)]
struct Settings {
    collection: Option<PathBuf>,
}

fn load_settings(path: &std::path::Path) -> Settings {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_settings(path: &std::path::Path, settings: &Settings) -> Result<(), String> {
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text + "\n").map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CollectionView {
    name: String,
    root: String,
    tree: Vec<Node>,
}

fn view(c: &Collection) -> Result<CollectionView, String> {
    Ok(CollectionView { name: c.name(), root: c.root().display().to_string(), tree: c.tree().map_err(|e| e.to_string())? })
}

/// Opens the system folder picker from Rust, so the UI never gets a general
/// file dialog or file system access of its own.
#[tauri::command]
async fn pick_collection(state: State<'_, AppState>) -> Result<Option<CollectionView>, String> {
    let Some(folder) = rfd::AsyncFileDialog::new().set_title("Open a folder for your requests").pick_folder().await else {
        return Ok(None);
    };
    let c = Collection::open(folder.path()).map_err(|e| e.to_string())?;
    save_settings(&state.settings_path, &Settings { collection: Some(c.root().to_path_buf()) })?;
    let shown = view(&c)?;
    *state.collection.lock().expect("collection lock") = Some(c);
    Ok(Some(shown))
}

#[tauri::command]
async fn collection_tree(state: State<'_, AppState>) -> Result<Option<CollectionView>, String> {
    let open = state.collection.lock().expect("collection lock").clone();
    open.as_ref().map(view).transpose()
}

#[tauri::command]
async fn close_collection(state: State<'_, AppState>) -> Result<(), String> {
    *state.collection.lock().expect("collection lock") = None;
    save_settings(&state.settings_path, &Settings::default())
}

#[tauri::command]
async fn read_request(state: State<'_, AppState>, path: String) -> Result<ReadRequest, String> {
    state.collection()?.read(&path).map_err(|e| e.to_string())
}

/// Refuses to write literal secrets unless `allow_secrets` is set; the
/// findings go back to the UI so it can offer to move them to the keyring.
#[tauri::command]
async fn save_request(state: State<'_, AppState>, path: String, request: SavedRequest, allow_secrets: bool) -> Result<SaveResult, String> {
    state.collection()?.save(&path, &request, allow_secrets).map_err(|e| e.to_string())
}

#[tauri::command]
async fn new_request_path(state: State<'_, AppState>, folder: String, name: String) -> Result<String, String> {
    state.collection()?.new_path(&folder, &name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_folder(state: State<'_, AppState>, parent: String, name: String) -> Result<String, String> {
    state.collection()?.create_folder(&parent, &name).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_request(state: State<'_, AppState>, path: String) -> Result<(), String> {
    state.collection()?.delete(&path).map_err(|e| e.to_string())
}

/// What the UI gets: the environments, which secrets have a value, and why
/// that could not be checked if the keyring did not answer.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvList {
    active: Option<String>,
    environments: Vec<Environment>,
    /// `"<env id>/<name>"` for each secret that has a value.
    secrets_set: Vec<String>,
    keyring_problem: Option<String>,
}

/// Keyring calls can wait on a D-Bus prompt, so they run off the async threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

async fn env_list(state: &AppState) -> Result<EnvList, String> {
    let file = state.envs();
    let store = Arc::clone(&state.secrets);
    let names: Vec<(String, String)> = file
        .environments
        .iter()
        .flat_map(|e| e.vars.iter().filter(|v| v.secret).map(move |v| (e.id.clone(), v.name.clone())))
        .collect();
    let (secrets_set, keyring_problem) = blocking(move || {
        let mut set = Vec::new();
        for (id, name) in names {
            match store.get(&id, &name) {
                Ok(Some(_)) => set.push(format!("{id}/{name}")),
                Ok(None) => {}
                Err(e) => return (set, Some(e.0)),
            }
        }
        (set, None)
    })
    .await?;
    Ok(EnvList { active: file.active, environments: file.environments, secrets_set, keyring_problem })
}

#[tauri::command]
async fn list_environments(state: State<'_, AppState>) -> Result<EnvList, String> {
    env_list(&state).await
}

/// Saves everything but secret values, and removes keyring entries for
/// secrets that were deleted, renamed or turned into plain values.
#[tauri::command]
async fn save_environments(state: State<'_, AppState>, file: EnvFile) -> Result<EnvList, String> {
    let file = file.without_secret_values();
    file.validate().map_err(|e| e.to_string())?;
    let dropped = state.envs().secrets_dropped_by(&file);
    let store = Arc::clone(&state.secrets);
    blocking(move || dropped.iter().try_for_each(|(id, name)| store.delete(id, name)))
        .await?
        .map_err(|e| format!("Could not clean up the keyring: {e}"))?;
    file.save(&state.env_path).map_err(|e| e.to_string())?;
    *state.envs.lock().expect("environments lock") = file;
    env_list(&state).await
}

/// Stores a secret value in the keyring; an empty value removes it.
#[tauri::command]
async fn set_secret(state: State<'_, AppState>, env_id: String, name: String, value: String) -> Result<EnvList, String> {
    let is_secret = state.envs().get(&env_id).is_some_and(|e| e.vars.iter().any(|v| v.secret && v.name == name));
    if !is_secret {
        return Err(format!("{{{{{name}}}}} is not a secret in this environment. Save the environment first."));
    }
    let store = Arc::clone(&state.secrets);
    blocking(move || if value.is_empty() { store.delete(&env_id, &name) } else { store.set(&env_id, &name, &value) })
        .await?
        .map_err(|e| format!("Could not save to the system keyring: {e}"))?;
    env_list(&state).await
}

/// Errors cross to the UI as the plain sentences from `SendError`, with any
/// secret values in them hidden.
#[tauri::command]
async fn send_request(
    state: State<'_, AppState>,
    spec: RequestSpec,
    env_id: Option<String>,
    confirmed: bool,
) -> Result<Response, String> {
    let env = env_id.and_then(|id| state.envs().get(&id).cloned());
    if let Some(e) = &env
        && e.production
        && !confirmed
        && WRITE_METHODS.contains(&spec.method.trim().to_ascii_uppercase().as_str())
    {
        return Err(format!("{} is a production environment. Confirm before sending {}.", e.name, spec.method));
    }
    let store = Arc::clone(&state.secrets);
    let resolved = blocking(move || resolve(&spec, env.as_ref(), store.as_ref())).await?.map_err(|e| e.to_string())?;
    match crane_core::send(&state.client, &resolved.spec, REQUEST_TIMEOUT).await {
        Ok(res) => Ok(mask_response(res, &resolved.secrets)),
        Err(e) => Err(mask(&e.to_string(), &resolved.secrets)),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let client = reqwest::Client::builder()
        .user_agent(concat!("Crane/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("the HTTP client could not start");
    tauri::Builder::default()
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
            let env_path = data_dir.join(ENV_FILE);
            let settings_path = data_dir.join(SETTINGS_FILE);
            // The last folder comes back if it still exists; if not, start without one.
            let collection = load_settings(&settings_path).collection.and_then(|p| Collection::open(&p).ok());
            // A damaged file is reported and left alone, never overwritten with nothing.
            let envs = EnvFile::load(&env_path).map_err(|e| e.to_string())?;
            app.manage(AppState {
                client,
                envs: Mutex::new(envs),
                env_path,
                secrets: Arc::new(secrets::KeyringSecrets),
                collection: Mutex::new(collection),
                settings_path,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            send_request,
            list_environments,
            save_environments,
            set_secret,
            pick_collection,
            collection_tree,
            close_collection,
            read_request,
            save_request,
            new_request_path,
            create_folder,
            delete_request,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Crane");
}
