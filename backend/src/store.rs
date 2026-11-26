use crate::config;
use crate::types::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, oneshot, Semaphore};
use tokio_util::sync::CancellationToken;

const MAX_MESSAGES: usize = 600;
const MAX_ACTIVITY: usize = 1500;

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

static ID_SEQ: AtomicU64 = AtomicU64::new(0);
pub fn new_id() -> String {
    let n = ID_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{:x}{:03x}", now_ms() % 0xffff_ffff, (n + fastrand::u64(..4096)) % 4096)
}

pub struct Store {
    inner: Mutex<CompanyState>,
    tx: broadcast::Sender<String>,
    dirty: AtomicBool,
    waiters: Mutex<HashMap<String, oneshot::Sender<Approval>>>,
    /// One cancellation token per running project.
    pub cancels: Mutex<HashMap<String, CancellationToken>>,
    /// Serialises jobs per employee (one person does one thing at a time).
    emp_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// Limits concurrent LLM calls.
    pub llm_slots: Semaphore,
    llm_slots_total: Mutex<u32>,
    /// Last time each employee read their inbox.
    pub inbox_read: Mutex<HashMap<String, u64>>,
    /// Operations (live projects): cancellation per project, cases / projects currently being handled.
    pub ops_cancels: Mutex<HashMap<String, CancellationToken>>,
    pub ops_busy: Mutex<std::collections::HashSet<String>>,
    pub data_dir: PathBuf,
    pub workspace_dir: PathBuf,
}

pub type S = Arc<Store>;

impl Store {
    pub fn load(root: &Path) -> S {
        let data_dir = root.join("data");
        let workspace_dir = root.join("workspace");
        let _ = std::fs::create_dir_all(&data_dir);
        let _ = std::fs::create_dir_all(&workspace_dir);
        let file = data_dir.join("state.json");
        let mut st: CompanyState = std::fs::read_to_string(&file)
            .ok()
            .and_then(|s| match serde_json::from_str(&s) {
                Ok(v) => Some(v),
                Err(e) => {
                    eprintln!("Không đọc được state.json ({e}), dùng trạng thái mới");
                    None
                }
            })
            .unwrap_or_default();
        if st.employees.is_empty() && st.projects.is_empty() {
            st.employees = config::default_employees();
        }
        // Nothing is running after a restart.
        for p in &mut st.projects {
            if p.status == ProjectStatus::Running {
                p.status = ProjectStatus::Paused;
            }
        }
        for a in &mut st.approvals {
            if a.status == ApprovalStatus::Pending {
                a.status = ApprovalStatus::Denied;
            }
        }
        let slots = st.settings.max_concurrent.max(1);
        let (tx, _) = broadcast::channel(1024);
        Arc::new(Store {
            inner: Mutex::new(st),
            tx,
            dirty: AtomicBool::new(false),
            waiters: Mutex::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            emp_locks: Mutex::new(HashMap::new()),
            llm_slots: Semaphore::new(slots as usize),
            llm_slots_total: Mutex::new(slots),
            inbox_read: Mutex::new(HashMap::new()),
            ops_cancels: Mutex::new(HashMap::new()),
            ops_busy: Mutex::new(std::collections::HashSet::new()),
            data_dir,
            workspace_dir,
        })
    }

    /// Read (or mutate) state without emitting events.
    pub fn with<R>(&self, f: impl FnOnce(&mut CompanyState) -> R) -> R {
        let mut g = self.inner.lock().unwrap();
        f(&mut g)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    pub fn emit(&self, e: ServerEvent) {
        if !matches!(e, ServerEvent::Handoff { .. } | ServerEvent::Meeting { .. } | ServerEvent::Runtime { .. }) {
            self.dirty.store(true, Ordering::Relaxed);
        }
        if let Ok(s) = serde_json::to_string(&e) {
            let _ = self.tx.send(s);
        }
    }

    pub fn snapshot_event(&self) -> String {
        let providers = ProviderId::ALL.iter().map(|p| (*p, config::provider_ready(*p))).collect();
        let state = self.with(|s| s.clone());
        serde_json::to_string(&ServerEvent::State { state, providers }).unwrap_or_default()
    }

    pub fn save_if_dirty(&self) {
        if !self.dirty.swap(false, Ordering::Relaxed) {
            return;
        }
        let json = self.with(|s| {
            let mut c = s.clone();
            c.runtime.clear();
            serde_json::to_string_pretty(&c)
        });
        if let Ok(json) = json {
            let file = self.data_dir.join("state.json");
            let tmp = self.data_dir.join("state.json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &file);
            }
        }
    }

    pub fn resize_llm_slots(&self, want: u32) {
        let want = want.clamp(1, 32);
        let mut total = self.llm_slots_total.lock().unwrap();
        if want > *total {
            self.llm_slots.add_permits((want - *total) as usize);
        } else if want < *total {
            self.llm_slots.forget_permits((*total - want) as usize);
        }
        *total = want;
    }

    pub fn emp_lock(&self, id: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.emp_locks.lock().unwrap().entry(id.to_string()).or_default().clone()
    }

    // ---------- typed helpers ----------

    pub fn employee(&self, id: &str) -> Option<Employee> {
        self.with(|s| s.employees.iter().find(|e| e.id == id).cloned())
    }

    pub fn staff(&self, role: Role) -> Vec<Employee> {
        self.with(|s| s.employees.iter().filter(|e| e.enabled && e.role == role).cloned().collect())
    }

    pub fn display_name(&self, id: &str) -> String {
        match id {
            "chairman" => "Chủ tịch".into(),
            "system" => "Hệ thống".into(),
            "all" => "Mọi người".into(),
            _ => self
                .employee(id)
                .map(|e| format!("{} ({})", e.name, config::role_label(e.role)))
                .unwrap_or_else(|| id.to_string()),
        }
    }

    pub fn set_runtime(&self, id: &str, status: EmpStatus, note: impl Into<String>, project_id: Option<&str>, ticket_id: Option<&str>) {
        let rt = EmpRuntime {
            status,
            note: note.into(),
            project_id: project_id.map(str::to_string),
            ticket_id: ticket_id.map(str::to_string),
        };
        self.with(|s| s.runtime.insert(id.to_string(), rt.clone()));
        self.emit(ServerEvent::Runtime { id: id.to_string(), runtime: rt });
    }

    pub fn upsert_employee(&self, e: Employee) {
        self.with(|s| match s.employees.iter_mut().find(|x| x.id == e.id) {
            Some(x) => *x = e.clone(),
            None => s.employees.push(e.clone()),
        });
        self.emit(ServerEvent::Upsert { coll: "employees", item: serde_json::to_value(&e).unwrap() });
    }

    /// Hire a new employee for a role (Mock provider, sensible default model and price).
    pub fn hire(&self, role: Role, name: Option<String>) -> Employee {
        let n = self.with(|s| s.employees.len());
        let model = match role {
            Role::Ceo | Role::Architect => "claude-opus-5-5",
            Role::Tester | Role::Secretary | Role::Devops | Role::Support | Role::Hr | Role::Accountant => "claude-haiku-5-5",
            _ => "claude-sonnet-5-5",
        };
        let (i, o) = config::known_price(model);
        const NAMES: [&str; 20] = [
            "An", "Bình", "Chi", "Dũng", "Giang", "Hà", "Khoa", "Linh", "Nam", "Phúc", "Quân", "Thảo", "Uyên", "Vinh", "Xuân", "Yến", "Sơn", "Tâm", "Mai", "Long",
        ];
        let taken: Vec<String> = self.with(|s| s.employees.iter().map(|e| e.name.clone()).collect());
        let auto = NAMES.iter().cycle().skip(n).take(NAMES.len()).find(|x| !taken.iter().any(|t| t == *x)).unwrap_or(&NAMES[0]);
        let e = Employee {
            id: new_id(),
            name: name.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| auto.to_string()),
            role,
            provider: ProviderId::Mock,
            model: model.into(),
            effort: "medium".into(),
            input_price: i,
            output_price: o,
            base_url: String::new(),
            extra_prompt: String::new(),
            color: config::COLORS[n % config::COLORS.len()].into(),
            enabled: true,
        };
        self.upsert_employee(e.clone());
        e
    }

    pub fn remove_employee(&self, id: &str) {
        self.with(|s| s.employees.retain(|e| e.id != id));
        self.emit(ServerEvent::Remove { coll: "employees", id: id.to_string() });
    }

    /// The folder a project's files live in: the customer's own folder when working in place.
    pub fn dir_of(&self, p: &Project) -> PathBuf {
        if p.dir.trim().is_empty() {
            self.workspace_dir.join(&p.slug)
        } else {
            PathBuf::from(&p.dir)
        }
    }

    pub fn project(&self, id: &str) -> Option<Project> {
        self.with(|s| s.projects.iter().find(|p| p.id == id).cloned())
    }

    /// Mutate a project and broadcast it.
    pub fn update_project<R>(&self, id: &str, f: impl FnOnce(&mut Project) -> R) -> Option<R> {
        let out = self.with(|s| {
            s.projects.iter_mut().find(|p| p.id == id).map(|p| {
                let r = f(p);
                p.updated_at = now_ms();
                (r, p.clone())
            })
        });
        out.map(|(r, p)| {
            self.emit(ServerEvent::Upsert { coll: "projects", item: serde_json::to_value(&p).unwrap() });
            r
        })
    }

    pub fn insert_project(&self, p: Project) {
        self.with(|s| s.projects.push(p.clone()));
        self.emit(ServerEvent::Upsert { coll: "projects", item: serde_json::to_value(&p).unwrap() });
    }

    pub fn ticket(&self, id: &str) -> Option<Ticket> {
        self.with(|s| s.tickets.iter().find(|t| t.id == id).cloned())
    }

    pub fn tickets_of(&self, project_id: &str) -> Vec<Ticket> {
        self.with(|s| s.tickets.iter().filter(|t| t.project_id == project_id).cloned().collect())
    }

    pub fn insert_ticket(&self, t: Ticket) {
        self.with(|s| s.tickets.push(t.clone()));
        self.emit(ServerEvent::Upsert { coll: "tickets", item: serde_json::to_value(&t).unwrap() });
    }

    pub fn update_ticket<R>(&self, id: &str, f: impl FnOnce(&mut Ticket) -> R) -> Option<R> {
        let out = self.with(|s| {
            s.tickets.iter_mut().find(|t| t.id == id).map(|t| {
                let r = f(t);
                t.updated_at = now_ms();
                (r, t.clone())
            })
        });
        out.map(|(r, t)| {
            self.emit(ServerEvent::Upsert { coll: "tickets", item: serde_json::to_value(&t).unwrap() });
            r
        })
    }

    pub fn post_message(&self, project_id: Option<&str>, from: &str, to: &str, text: impl Into<String>, kind: &str) -> ChatMsg {
        let m = ChatMsg {
            id: new_id(),
            ts: now_ms(),
            project_id: project_id.map(str::to_string),
            from: from.into(),
            to: to.into(),
            text: text.into(),
            kind: kind.into(),
        };
        self.with(|s| {
            s.messages.push(m.clone());
            if s.messages.len() > MAX_MESSAGES {
                let n = s.messages.len() - MAX_MESSAGES;
                s.messages.drain(..n);
            }
        });
        self.emit(ServerEvent::Upsert { coll: "messages", item: serde_json::to_value(&m).unwrap() });
        m
    }

    pub fn log_activity(&self, employee_id: &str, project_id: Option<&str>, kind: &str, text: impl AsRef<str>) {
        let text: String = text.as_ref().chars().take(4000).collect();
        let a = Activity {
            id: new_id(),
            ts: now_ms(),
            employee_id: employee_id.into(),
            project_id: project_id.map(str::to_string),
            kind: kind.into(),
            text,
        };
        self.with(|s| {
            s.activity.push(a.clone());
            if s.activity.len() > MAX_ACTIVITY {
                let n = s.activity.len() - MAX_ACTIVITY;
                s.activity.drain(..n);
            }
        });
        self.emit(ServerEvent::Upsert { coll: "activity", item: serde_json::to_value(&a).unwrap() });
    }

    pub fn add_usage(&self, employee_id: &str, project_id: Option<&str>, input: u64, output: u64) {
        let (usage, project) = self.with(|s| {
            let cost = s
                .employees
                .iter()
                .find(|e| e.id == employee_id)
                .filter(|e| e.provider != ProviderId::Mock)
                .map(|e| (input as f64 * e.input_price + output as f64 * e.output_price) / 1_000_000.0)
                .unwrap_or(0.0);
            let u = s.usage.entry(employee_id.to_string()).or_default();
            u.calls += 1;
            u.input_tokens += input;
            u.output_tokens += output;
            u.cost_usd += cost;
            let u = u.clone();
            let p = project_id.and_then(|pid| s.projects.iter_mut().find(|p| p.id == pid)).map(|p| {
                p.spent_usd += cost;
                p.updated_at = now_ms();
                p.clone()
            });
            (u, p)
        });
        self.emit(ServerEvent::Usage { id: employee_id.into(), usage });
        if let Some(p) = project {
            self.emit(ServerEvent::Upsert { coll: "projects", item: serde_json::to_value(&p).unwrap() });
        }
    }

    pub fn case(&self, id: &str) -> Option<SupportCase> {
        self.with(|s| s.cases.iter().find(|c| c.id == id).cloned())
    }

    pub fn insert_case(&self, c: SupportCase) {
        self.with(|s| s.cases.push(c.clone()));
        self.emit(ServerEvent::Upsert { coll: "cases", item: serde_json::to_value(&c).unwrap() });
    }

    pub fn update_case<R>(&self, id: &str, f: impl FnOnce(&mut SupportCase) -> R) -> Option<R> {
        let out = self.with(|s| {
            s.cases.iter_mut().find(|c| c.id == id).map(|c| {
                let r = f(c);
                c.updated_at = now_ms();
                (r, c.clone())
            })
        });
        out.map(|(r, c)| {
            self.emit(ServerEvent::Upsert { coll: "cases", item: serde_json::to_value(&c).unwrap() });
            r
        })
    }

    pub fn insert_deal(&self, d: Deal) {
        self.with(|s| s.deals.push(d.clone()));
        self.emit(ServerEvent::Upsert { coll: "deals", item: serde_json::to_value(&d).unwrap() });
    }

    /// Cancellation token for a live project's operations (created on demand).
    pub fn ops_token(&self, pid: &str) -> CancellationToken {
        self.ops_cancels.lock().unwrap().entry(pid.to_string()).or_default().clone()
    }

    pub fn stop_ops(&self, pid: &str) {
        if let Some(t) = self.ops_cancels.lock().unwrap().remove(pid) {
            t.cancel();
        }
    }

    /// Mark a unit of operations work as busy; returns false if it already was.
    pub fn try_busy(&self, key: &str) -> bool {
        self.ops_busy.lock().unwrap().insert(key.to_string())
    }

    pub fn done_busy(&self, key: &str) {
        self.ops_busy.lock().unwrap().remove(key);
    }

    /// Update an employee's KPI counters and broadcast them.
    pub fn kpi(&self, employee_id: &str, f: impl FnOnce(&mut Kpi)) {
        let k = self.with(|s| {
            let k = s.kpi.entry(employee_id.to_string()).or_default();
            f(k);
            k.last_active = now_ms();
            k.clone()
        });
        self.emit(ServerEvent::Kpi { id: employee_id.into(), kpi: k });
    }

    // ---------- approvals: blocking questions to the chairman ----------

    pub async fn request_approval(
        &self,
        project_id: Option<&str>,
        employee_id: &str,
        kind: ApprovalKind,
        title: impl Into<String>,
        detail: impl Into<String>,
        cancel: &CancellationToken,
    ) -> Result<Approval, String> {
        let a = Approval {
            id: new_id(),
            ts: now_ms(),
            project_id: project_id.map(str::to_string),
            employee_id: employee_id.into(),
            kind,
            title: title.into(),
            detail: detail.into(),
            status: ApprovalStatus::Pending,
            answer: None,
        };
        let (tx, rx) = oneshot::channel();
        self.waiters.lock().unwrap().insert(a.id.clone(), tx);
        self.with(|s| s.approvals.push(a.clone()));
        self.emit(ServerEvent::Upsert { coll: "approvals", item: serde_json::to_value(&a).unwrap() });
        tokio::select! {
            r = rx => r.map_err(|_| "approval dropped".to_string()),
            _ = cancel.cancelled() => {
                self.waiters.lock().unwrap().remove(&a.id);
                self.finish_approval(&a.id, ApprovalStatus::Denied, None);
                Err("aborted".into())
            }
        }
    }

    fn finish_approval(&self, id: &str, status: ApprovalStatus, answer: Option<String>) -> Option<Approval> {
        let a = self.with(|s| {
            s.approvals.iter_mut().find(|a| a.id == id && a.status == ApprovalStatus::Pending).map(|a| {
                a.status = status;
                a.answer = answer;
                a.clone()
            })
        })?;
        self.emit(ServerEvent::Upsert { coll: "approvals", item: serde_json::to_value(&a).unwrap() });
        Some(a)
    }

    pub fn resolve_approval(&self, id: &str, approve: bool, answer: Option<String>) -> Option<Approval> {
        let kind = self.with(|s| s.approvals.iter().find(|a| a.id == id).map(|a| a.kind))?;
        let status = match (kind, approve) {
            (ApprovalKind::Question, true) => ApprovalStatus::Answered,
            (_, true) => ApprovalStatus::Approved,
            (_, false) => ApprovalStatus::Denied,
        };
        let a = self.finish_approval(id, status, answer)?;
        if let Some(tx) = self.waiters.lock().unwrap().remove(id) {
            let _ = tx.send(a.clone());
        }
        Some(a)
    }
}
