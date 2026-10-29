mod prompt;
use crate::{
    api::{Clock, Event, Events},
    devices::{DeviceId, DeviceManager},
};
use parking_lot::Mutex;
#[cfg(any(test, feature = "test-hooks"))]
pub use prompt::FakePrompter;
pub use prompt::{ApprovalPrompter, NativePrompter, NoopNotifier, TrayNotifier};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use td_core::layout::{Layout, Page};
use td_host::{actions::Plan, approval::CommandSpec};
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ActionSpec {
    pub kind: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub shell: bool,
}
impl ActionSpec {
    pub fn hash(&self) -> String {
        Sha256::digest(serde_json::to_vec(self).expect("action serializes"))
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
    fn command(&self) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: self.args.clone(),
            cwd: self.cwd.clone(),
            shell: self.shell,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ApprovalRequest {
    pub id: String,
    pub device: DeviceId,
    pub page: String,
    pub tile: String,
    pub hash: String,
    #[serde(flatten)]
    pub spec: ActionSpec,
    pub created_ms: u64,
}
impl Event for ApprovalRequest {
    const NAME: &'static str = "approval.requested";
    fn device(&self) -> Option<&str> {
        Some(&self.device)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Answer {
    Once,
    Always,
    Deny,
    Expired,
}
#[derive(Serialize, Deserialize, TS)]
pub struct ApprovalResolved {
    pub id: String,
    pub answer: Answer,
}
impl Event for ApprovalResolved {
    const NAME: &'static str = "approval.resolved";
}
#[derive(Clone, Serialize, Deserialize, TS)]
pub struct AllowedAction {
    pub hash: String,
    #[serde(flatten)]
    pub spec: ActionSpec,
    pub granted_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
pub struct ActionLog {
    pub id: String,
    pub device: DeviceId,
    pub at_ms: u64,
    pub page: String,
    pub tile: String,
    pub kind: String,
    pub status: String,
    pub detail: String,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
}
impl Event for ActionLog {
    const NAME: &'static str = "action.logged";
    fn device(&self) -> Option<&str> {
        Some(&self.device)
    }
}
struct Pending {
    request: ApprovalRequest,
    deadline: Instant,
    shown: Option<Instant>,
    context: String,
}
#[derive(Default)]
struct State {
    pending: VecDeque<Pending>,
    allowed: Vec<AllowedAction>,
    log: VecDeque<ActionLog>,
    serial: u64,
    load_failed: bool,
}
struct Inner {
    state: Mutex<State>,
    events: Events,
    clock: Clock,
    devices: DeviceManager,
    path: PathBuf,
    prompter: Arc<dyn ApprovalPrompter>,
    running: AtomicUsize,
    #[cfg(any(test, feature = "test-hooks"))]
    fake: Option<Arc<FakePrompter>>,
}
#[derive(Clone)]
pub struct Actions(Arc<Inner>);
impl Actions {
    pub(crate) fn new(
        events: Events,
        clock: Clock,
        devices: DeviceManager,
        path: PathBuf,
        mock: bool,
    ) -> Self {
        let mut state = State::default();
        match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(allowed) => state.allowed = allowed,
                Err(_) => state.load_failed = true,
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => state.load_failed = true,
        }
        let prompter: Arc<dyn ApprovalPrompter> = Arc::new(NativePrompter::default());
        #[cfg(any(test, feature = "test-hooks"))]
        let fake = (mock && std::env::var_os("TOUCHDECK_NATIVE_APPROVALS").is_none())
            .then(|| Arc::new(FakePrompter::default()));
        #[cfg(any(test, feature = "test-hooks"))]
        let prompter: Arc<dyn ApprovalPrompter> = fake.clone().map_or(prompter, |p| p);
        let _ = mock;
        let inner = Arc::new(Inner {
            state: Mutex::new(state),
            events,
            clock,
            devices: devices.clone(),
            path,
            prompter,
            running: AtomicUsize::new(0),
            #[cfg(any(test, feature = "test-hooks"))]
            fake,
        });
        let weak = Arc::downgrade(&inner);
        devices.action_handler(Arc::new(move |device, session, event| {
            if let Some(inner) = weak.upgrade() {
                Actions(inner).press(device, session, event);
            }
        }));
        let weak = Arc::downgrade(&inner);
        std::thread::spawn(move || {
            loop {
                let Some(inner) = weak.upgrade() else { break };
                let actions = Actions(inner);
                actions.prompt_next();
                drop(actions);
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        Self(inner)
    }
    pub fn pending(&self) -> Vec<ApprovalRequest> {
        self.0
            .state
            .lock()
            .pending
            .iter()
            .map(|p| p.request.clone())
            .collect()
    }
    pub fn allowed(&self) -> Vec<AllowedAction> {
        self.0.state.lock().allowed.clone()
    }
    pub fn log(&self, device: Option<&DeviceId>) -> Vec<ActionLog> {
        self.0
            .state
            .lock()
            .log
            .iter()
            .filter(|l| device.is_none_or(|d| d == &l.device))
            .cloned()
            .collect()
    }
    fn save(&self, allowed: &[AllowedAction]) -> std::io::Result<()> {
        if let Some(parent) = self.0.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = self.0.path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec(allowed)?)?;
        std::fs::rename(tmp, &self.0.path)
    }
    pub fn remove(&self, hash: &str) -> Result<bool, std::io::Error> {
        let mut state = self.0.state.lock();
        let mut next = state.allowed.clone();
        next.retain(|a| a.hash != hash);
        if next.len() == state.allowed.len() {
            return Ok(false);
        }
        self.save(&next)?;
        state.allowed = next;
        Ok(true)
    }
    pub fn show(&self, id: &str) -> Result<(), crate::features::actions::ActionError> {
        use crate::{api::CommonError, features::actions::ActionError};
        let mut state = self.0.state.lock();
        let p = state
            .pending
            .iter_mut()
            .find(|p| p.request.id == id && self.0.clock.now() < p.deadline)
            .ok_or(ActionError::ApprovalMissing)?;
        let now = self.0.clock.now();
        if p.shown
            .is_some_and(|t| now.duration_since(t) < Duration::from_secs(5))
        {
            return Err(CommonError::Limits(
                "Show prompt again is limited to once every 5 seconds.",
            )
            .into());
        }
        p.shown = Some(now);
        self.0.prompter.show(id);
        Ok(())
    }
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn script(&self, answers: Vec<prompt::ScriptAnswer>) -> bool {
        if let Some(fake) = &self.0.fake {
            fake.script(answers);
            true
        } else {
            false
        }
    }
    fn logged(
        &self,
        request: &ApprovalRequest,
        status: &str,
        detail: String,
        pid: Option<u32>,
        exit_code: Option<i32>,
    ) {
        let mut state = self.0.state.lock();
        state.serial += 1;
        let entry = ActionLog {
            id: format!("a{}", state.serial),
            device: request.device.clone(),
            at_ms: crate::now_ms(),
            page: request.page.clone(),
            tile: request.tile.clone(),
            kind: request.spec.kind.clone(),
            status: status.into(),
            detail,
            pid,
            exit_code,
        };
        self.0.events.publish(&entry);
        state.log.push_back(entry);
        while state.log.len() > 100 {
            state.log.pop_front();
        }
    }
    fn press(&self, device: &DeviceId, session: &td_link::Session, event: &serde_json::Value) {
        let page = event["page"].as_str().unwrap_or_default();
        let tile = event["tile"].as_str().unwrap_or_default();
        let resolve_device_layout_action = || -> Option<(Plan, String)> {
            let shown = session.layout_state(Duration::from_secs(2)).ok()?;
            let layout = match shown.source.as_str() {
                "builtin" => Layout::parse(include_bytes!(
                    "../../../../../schema/examples/builtin.json"
                ))
                .ok()?,
                "storage" => Layout::parse(
                    &td_link::xfer::get_layout(session, shown.orientation)
                        .ok()?
                        .data,
                )
                .ok()?,
                _ => return None,
            };
            let tile = td_host::actions::find_tile(&layout, page, tile)?;
            let title = layout
                .pages
                .iter()
                .find_map(|p| match p {
                    Page::Grid { id, title, .. } if id == page => Some(title.as_str()),
                    _ => None,
                })
                .unwrap_or(page);
            let name = self
                .0
                .devices
                .get(device)
                .and_then(|d| d.name)
                .unwrap_or_else(|| device.to_string());
            Some((
                Plan::of(&tile.action),
                format!("Device: {name}\nPage: {title}\nTile: {}", tile.label),
            ))
        };
        let resolved = resolve_device_layout_action();
        let mut request = ApprovalRequest {
            id: String::new(),
            device: device.clone(),
            page: page.into(),
            tile: tile.into(),
            hash: String::new(),
            spec: ActionSpec {
                kind: event["kind"].as_str().unwrap_or_default().into(),
                program: String::new(),
                args: vec![],
                cwd: String::new(),
                shell: false,
            },
            created_ms: crate::now_ms(),
        };
        let Some((plan, context)) = resolved else {
            self.logged(
                &request,
                "unknown",
                "No matching Tile in the Device's Layout.".into(),
                None,
                None,
            );
            return;
        };
        let approval = match plan {
            Plan::Deck => return, // Key and media reports are already typed by the Device's mirror.
            Plan::Launch { path, args, cwd } => {
                request.spec = ActionSpec {
                    kind: "launch".into(),
                    program: path,
                    args,
                    cwd,
                    shell: false,
                };
                true
            }
            Plan::Command(spec) => {
                request.spec = ActionSpec {
                    kind: "command".into(),
                    program: spec.program,
                    args: spec.args,
                    cwd: spec.cwd,
                    shell: spec.shell,
                };
                true
            }
            Plan::Open { target } => {
                let safe = target.split_once(':').is_some_and(|(scheme, _)| {
                    ["http", "https", "mailto"]
                        .iter()
                        .any(|s| scheme.eq_ignore_ascii_case(s))
                }) && !target.chars().any(char::is_control);
                request.spec.kind = "open".into();
                request.spec.program = target;
                !safe
            }
        };
        request.hash = request.spec.hash();
        let mut state = self.0.state.lock();
        if state.pending.iter().any(|p| {
            p.request.device == *device && p.request.page == page && p.request.tile == tile
        }) {
            return;
        }
        if !approval || state.allowed.iter().any(|a| a.hash == request.hash) {
            drop(state);
            self.execute(request);
            return;
        }
        if state.pending.len() >= 16 {
            drop(state);
            self.logged(&request, "denied", "limits.exceeded".into(), None, None);
            return;
        }
        state.serial += 1;
        request.id = format!("p{}", state.serial);
        self.0.events.publish(&request);
        state.pending.push_back(Pending {
            request,
            deadline: self.0.clock.now() + Duration::from_secs(300),
            shown: None,
            context,
        });
    }
    fn prompt_next(&self) {
        let next = self
            .0
            .state
            .lock()
            .pending
            .front()
            .map(|p| (p.request.clone(), p.deadline, p.context.clone()));
        let Some((request, deadline, context)) = next else {
            return;
        };
        let expired = || self.0.clock.now() >= deadline || Arc::strong_count(&self.0) == 1;
        let answer = if expired() {
            Answer::Expired
        } else {
            let _suppressed = self.0.devices.suppress_input();
            self.0.prompter.prompt(&request, &context, &expired)
        };
        let mut answer = if expired() { Answer::Expired } else { answer };
        let mut state = self.0.state.lock();
        if answer == Answer::Always {
            let mut allowed = state.allowed.clone();
            allowed.retain(|a| a.hash != request.hash);
            allowed.push(AllowedAction {
                hash: request.hash.clone(),
                spec: request.spec.clone(),
                granted_ms: crate::now_ms(),
            });
            if state.load_failed || self.save(&allowed).is_err() {
                answer = Answer::Deny;
            } else {
                state.allowed = allowed;
            }
        }
        state.pending.pop_front();
        self.0.events.publish(&ApprovalResolved {
            id: request.id.clone(),
            answer,
        });
        drop(state);
        match answer {
            Answer::Once | Answer::Always => self.execute(request),
            Answer::Deny => self.logged(
                &request,
                "denied",
                "Denied on this PC, unavailable native prompt, or allowed-actions storage failure."
                    .into(),
                None,
                None,
            ),
            Answer::Expired => {
                self.logged(&request, "expired", "Approval expired.".into(), None, None)
            }
        }
    }
    fn execute(&self, request: ApprovalRequest) {
        if self.0.running.fetch_add(1, Ordering::SeqCst) >= 16 {
            self.0.running.fetch_sub(1, Ordering::SeqCst);
            self.logged(&request, "denied", "limits.exceeded".into(), None, None);
            return;
        }
        let actions = self.clone();
        std::thread::spawn(move || {
            let spec = &request.spec;
            let result = match spec.kind.as_str() {
                "launch" => td_host::actions::launch(&spec.program, &spec.args, &spec.cwd)
                    .map(|pid| ("started", Some(pid), None)),
                "open" => td_host::actions::open(&spec.program).map(|()| ("opened", None, None)),
                _ => {
                    td_host::actions::run(&spec.command()).map(|exit| ("exited", None, exit.code()))
                }
            };
            match result {
                Ok((status, pid, exit)) => {
                    actions.logged(&request, status, spec.program.clone(), pid, exit)
                }
                Err(e) => actions.logged(&request, "failed", e.to_string(), None, None),
            }
            actions.0.running.fetch_sub(1, Ordering::SeqCst);
        });
    }
}
pub use prompt::ScriptAnswer;
