use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Ceo,
    Pm,
    Ba,
    Architect,
    Dev,
    Reviewer,
    Tester,
    Devops,
    Designer,
    Secretary,
    Marketing,
    Sales,
    Support,
    Hr,
    Accountant,
    DataEngineer,
    Security,
    Legal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Mock,
    Anthropic,
    Openai,
    Openrouter,
    Gemini,
    Ollama,
    Custom,
}

impl ProviderId {
    pub const ALL: [ProviderId; 7] = [
        ProviderId::Mock,
        ProviderId::Anthropic,
        ProviderId::Openai,
        ProviderId::Openrouter,
        ProviderId::Gemini,
        ProviderId::Ollama,
        ProviderId::Custom,
    ];
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Employee {
    pub id: String,
    pub name: String,
    pub role: Role,
    pub provider: ProviderId,
    pub model: String,
    /// "" | low | medium | high | xhigh | max
    #[serde(default)]
    pub effort: String,
    /// USD per 1M tokens
    #[serde(default)]
    pub input_price: f64,
    #[serde(default)]
    pub output_price: f64,
    /// Only for provider "custom" (OpenAI-compatible endpoint).
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub extra_prompt: String,
    /// Custom skill text / Markdown configured per employee.
    #[serde(default)]
    pub skill_text: String,
    /// Optional path to a `.md` skill file (relative to root / `skills/`, or absolute).
    #[serde(default)]
    pub skill_file: String,
    pub color: String,
    /// "" (guess from the name) | "male" | "female": how the character looks in the office.
    #[serde(default)]
    pub gender: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EmpStatus {
    #[default]
    Idle,
    Thinking,
    Working,
    Meeting,
    Waiting,
    Blocked,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmpRuntime {
    pub status: EmpStatus,
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Todo,
    InProgress,
    Review,
    Testing,
    Done,
    Blocked,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    pub id: String,
    pub project_id: String,
    pub key: String,
    pub title: String,
    pub description: String,
    pub acceptance: Vec<String>,
    /// feature | bug | task
    #[serde(rename = "type")]
    pub kind: String,
    pub status: TicketStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    pub created_by: String,
    pub bounces: u32,
    pub notes: Vec<String>,
    /// 1 = cao, 2 = vừa, 3 = thấp (PM đặt).
    #[serde(default = "default_priority")]
    pub priority: u8,
    pub created_at: u64,
    pub updated_at: u64,
}

fn default_priority() -> u8 {
    2
}

/// Phases run in this order; phases whose role is missing are skipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Plan,
    Requirements,
    Planning,
    Design,
    Architecture,
    Build,
    Release,
    Report,
    AwaitingApproval,
    /// Approved: the business departments prepare the launch.
    Launch,
    /// Live: customers, support, maintenance, campaigns.
    Operating,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Running,
    Paused,
    Stopped,
    AwaitingApproval,
    Operating,
    Done,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub request: String,
    /// Change requests from the chairman, newest last.
    pub feedback: Vec<String>,
    pub phase: Phase,
    pub status: ProjectStatus,
    pub budget_usd: f64,
    pub spent_usd: f64,
    pub report: String,
    pub error: String,
    pub ticket_seq: u32,
    /// How many times DevOps sent the build back with bugs (this round).
    #[serde(default)]
    pub release_rounds: u32,
    /// Design reviews done in this round of work.
    #[serde(default)]
    pub design_reviews: u32,
    /// Whole-product QA passes done in this round of work.
    #[serde(default)]
    pub qa_rounds: u32,
    /// Security reviews done in this round of work.
    #[serde(default)]
    pub security_reviews: u32,
    /// Automatic restarts after a failure (reset when a run completes).
    #[serde(default)]
    pub auto_retries: u32,
    /// When a pending automatic restart is due (ms); 0 = none. Saved so a server restart keeps it.
    #[serde(default)]
    pub retry_at: u64,
    /// The chairman told the team to decide alone on this project (no approvals).
    #[serde(default)]
    pub autopilot: bool,
    /// Design directions to offer for this project; 0 = the company setting.
    #[serde(default)]
    pub design_options: u8,
    /// Employees working on this project (picked when it starts, so parallel projects don't share people).
    #[serde(default)]
    pub team: Vec<String>,
    /// Rounds the hand-over gate sent the work back (blocked tickets / machine-found problems).
    #[serde(default)]
    pub gate_rounds: u32,
    /// Meeting minutes written so far (numbering for docs/meetings/).
    #[serde(default)]
    pub meetings: u32,
    /// Technology the chairman asked for ("" = the CEO asks at kickoff).
    #[serde(default)]
    pub tech: String,
    /// Frozen source snapshots (hand-overs, releases), oldest first.
    #[serde(default)]
    pub versions: Vec<ProjectVersion>,
    /// Built on the customer's existing source code (uploaded zip or Git) instead of from scratch.
    #[serde(default)]
    pub existing: bool,
    /// Where that code came from (file name or Git URL).
    #[serde(default)]
    pub source: String,
    /// Absolute folder on this computer the team works in directly ("" = workspace/<slug>).
    #[serde(default)]
    pub dir: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVersion {
    pub n: u32,
    pub label: String,
    pub ts: u64,
    pub size: u64,
    pub files: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMsg {
    pub id: String,
    pub ts: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// employee id, "chairman" or "system"
    pub from: String,
    /// employee id, "all" or "chairman"
    pub to: String,
    pub text: String,
    /// handoff | chat | report | system | question
    pub kind: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    Command,
    Budget,
    Question,
    /// Sales wants to close a deal (revenue is recorded only when the chairman confirms).
    Deal,
    /// HR proposes hiring someone.
    Hire,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Answered,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: String,
    pub ts: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub employee_id: String,
    pub kind: ApprovalKind,
    pub title: String,
    pub detail: String,
    pub status: ApprovalStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// Choices shown as cards (e.g. design directions with screenshots); the answer names the chosen id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<ApprovalOption>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalOption {
    /// Short id the chairman picks ("a", "b", "c").
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub note: String,
    /// Images inside the project folder (relative paths, served under /preview/<project>/).
    #[serde(default)]
    pub images: Vec<String>,
    /// Page to open for this option (relative path in the project folder).
    #[serde(default)]
    pub link: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub id: String,
    pub ts: u64,
    pub employee_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// llm | tool | result | error
    pub kind: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UsageStat {
    pub calls: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

/// Performance counters per employee (for the KPI board).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Kpi {
    /// Jobs finished successfully / ended with an error.
    pub jobs: u64,
    pub errors: u64,
    /// Total time spent on jobs (ms).
    pub work_ms: u64,
    /// Dev: tickets that passed QA / tickets sent back.
    pub tickets_done: u64,
    pub tickets_returned: u64,
    /// Tester / DevOps: bugs found.
    pub bugs_found: u64,
    /// Reviewer: reviews done / changes requested.
    pub reviews: u64,
    pub changes_requested: u64,
    /// Tickets created (BA/PM/Tech Lead).
    pub tickets_created: u64,
    /// Documents written (any file write).
    pub files_written: u64,
    /// Support / Sales.
    pub cases_resolved: u64,
    pub replies: u64,
    pub csat_sum: u64,
    pub csat_count: u64,
    pub deals_won: u64,
    pub revenue_vnd: u64,
    /// Marketing campaigns, reports, documents produced as business work.
    pub campaigns: u64,
    pub last_active: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseKind {
    Support,
    Bug,
    Sales,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    /// Needs a reply from the company.
    Open,
    InProgress,
    WaitingCustomer,
    /// Sent to the dev team as a bug ticket.
    Escalated,
    Resolved,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseMsg {
    /// "customer", "system", "chairman" or an employee id.
    pub from: String,
    pub text: String,
    pub ts: u64,
}

/// A conversation with a customer (support request, bug report or sales inquiry).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportCase {
    pub id: String,
    pub project_id: String,
    pub kind: CaseKind,
    pub status: CaseStatus,
    pub customer: String,
    #[serde(default)]
    pub contact: String,
    pub subject: String,
    pub messages: Vec<CaseMsg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticket_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<u8>,
    /// Secret for the customer's case link (shown to the chairman's UI and the customer only).
    #[serde(default)]
    pub token: String,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Revenue confirmed by the chairman.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deal {
    pub id: String,
    pub project_id: String,
    pub customer: String,
    pub plan: String,
    pub amount_vnd: u64,
    pub employee_id: String,
    pub ts: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub auto_approve_commands: bool,
    pub max_concurrent: u32,
    pub max_bounces: u32,
    pub max_agent_steps: u32,
    pub default_budget_usd: f64,
    pub mock_delay_ms: u64,
    /// Discussion rounds at the kickoff meeting before the CEO decides (0 = the CEO decides alone).
    pub meeting_rounds: u32,
    /// Model a stuck employee switches to (same provider / endpoint); "" = none.
    pub fallback_model: String,
    /// Times a failed project is restarted automatically (5 minutes apart).
    pub auto_retries: u32,
    /// Cheaper model for light jobs (votes, minutes, stand-ups); "" = everyone uses their own model.
    pub light_model: String,
    /// The chairman approves the design mockup before anything is built.
    pub approve_design: bool,
    /// The team decides alone: questions and design choices no longer wait for the chairman.
    pub autopilot: bool,
    /// How many design directions the designer offers the chairman (1 = a single design).
    pub design_options: u8,
    /// When enabled, every employee loads `tools_skill_file` (default `GEMINI.md`) into their system prompt.
    pub tools_mode: bool,
    /// Skill file loaded for all employees only when `tools_mode` is enabled.
    pub tools_skill_file: String,
    /// MCP servers switched on/off from the UI, overriding `enabled` in mcp.json (name -> on).
    pub mcp: HashMap<String, bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_approve_commands: false,
            max_concurrent: 3,
            max_bounces: 3,
            max_agent_steps: 30,
            default_budget_usd: 2.0,
            mock_delay_ms: 1200,
            meeting_rounds: 2,
            fallback_model: String::new(),
            auto_retries: 3,
            light_model: String::new(),
            approve_design: true,
            autopilot: false,
            design_options: 5,
            tools_mode: false,
            tools_skill_file: String::new(),
            mcp: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CompanyState {
    pub employees: Vec<Employee>,
    #[serde(skip_deserializing)]
    pub runtime: HashMap<String, EmpRuntime>,
    pub projects: Vec<Project>,
    pub tickets: Vec<Ticket>,
    pub messages: Vec<ChatMsg>,
    pub approvals: Vec<Approval>,
    pub activity: Vec<Activity>,
    pub usage: HashMap<String, UsageStat>,
    pub kpi: HashMap<String, Kpi>,
    pub cases: Vec<SupportCase>,
    pub deals: Vec<Deal>,
    pub settings: Settings,
}

/// Events pushed to the browser over WebSocket.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ServerEvent {
    State {
        state: CompanyState,
        providers: HashMap<ProviderId, bool>,
    },
    Upsert {
        coll: &'static str,
        item: serde_json::Value,
    },
    Remove {
        coll: &'static str,
        id: String,
    },
    Runtime {
        id: String,
        runtime: EmpRuntime,
    },
    Usage {
        id: String,
        usage: UsageStat,
    },
    Kpi {
        id: String,
        kpi: Kpi,
    },
    Settings {
        settings: Settings,
    },
    Handoff {
        from: String,
        to: String,
        text: String,
    },
    Meeting {
        project_id: String,
        participants: Vec<String>,
        topic: String,
        active: bool,
    },
    /// Something appeared on an employee's computer (code typed, a command run, a page opened).
    Screen {
        id: String,
        screen: serde_json::Value,
    },
}
