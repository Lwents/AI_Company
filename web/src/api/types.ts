export type Role =
  | "ceo"
  | "pm"
  | "ba"
  | "architect"
  | "dev"
  | "reviewer"
  | "tester"
  | "devops"
  | "designer"
  | "secretary"
  | "marketing"
  | "sales"
  | "support"
  | "hr"
  | "accountant"
  | "legal";
export type ProviderId = "mock" | "anthropic" | "openai" | "openrouter" | "gemini" | "ollama" | "custom";

export interface Employee {
  id: string;
  name: string;
  role: Role;
  provider: ProviderId;
  model: string;
  effort: string;
  inputPrice: number;
  outputPrice: number;
  baseUrl: string;
  extraPrompt: string;
  color: string;
  enabled: boolean;
}

export type EmpStatus = "idle" | "thinking" | "working" | "meeting" | "waiting" | "blocked";
export interface EmpRuntime {
  status: EmpStatus;
  note: string;
  projectId?: string;
  ticketId?: string;
}

export type TicketStatus = "todo" | "in_progress" | "review" | "testing" | "done" | "blocked";
export interface Ticket {
  id: string;
  projectId: string;
  key: string;
  title: string;
  description: string;
  acceptance: string[];
  type: string;
  status: TicketStatus;
  assignee?: string;
  createdBy: string;
  bounces: number;
  notes: string[];
  priority: number;
  createdAt: number;
  updatedAt: number;
}

export type Phase =
  | "plan"
  | "requirements"
  | "planning"
  | "design"
  | "architecture"
  | "build"
  | "release"
  | "report"
  | "awaiting_approval"
  | "launch"
  | "operating"
  | "done";
export type ProjectStatus = "running" | "paused" | "stopped" | "awaiting_approval" | "operating" | "done" | "failed";
export interface Project {
  id: string;
  name: string;
  slug: string;
  request: string;
  feedback: string[];
  phase: Phase;
  status: ProjectStatus;
  budgetUsd: number;
  spentUsd: number;
  report: string;
  error: string;
  ticketSeq: number;
  releaseRounds: number;
  meetings: number;
  createdAt: number;
  updatedAt: number;
}

export interface ChatMsg {
  id: string;
  ts: number;
  projectId?: string;
  from: string;
  to: string;
  text: string;
  kind: "handoff" | "chat" | "report" | "system" | "question";
}

export interface Approval {
  id: string;
  ts: number;
  projectId?: string;
  employeeId: string;
  kind: "command" | "budget" | "question" | "deal" | "hire";
  title: string;
  detail: string;
  status: "pending" | "approved" | "denied" | "answered";
  answer?: string;
}

export interface Activity {
  id: string;
  ts: number;
  employeeId: string;
  projectId?: string;
  kind: "llm" | "tool" | "result" | "error";
  text: string;
}

export interface UsageStat {
  calls: number;
  inputTokens: number;
  outputTokens: number;
  costUsd: number;
}

export interface Kpi {
  jobs: number;
  errors: number;
  workMs: number;
  ticketsDone: number;
  ticketsReturned: number;
  bugsFound: number;
  reviews: number;
  changesRequested: number;
  ticketsCreated: number;
  filesWritten: number;
  casesResolved: number;
  replies: number;
  csatSum: number;
  csatCount: number;
  dealsWon: number;
  revenueVnd: number;
  campaigns: number;
  lastActive: number;
}

export type CaseKind = "support" | "bug" | "sales";
export type CaseStatus = "open" | "in_progress" | "waiting_customer" | "escalated" | "resolved";

export interface CaseMsg {
  from: string;
  text: string;
  ts: number;
}

export interface SupportCase {
  id: string;
  projectId: string;
  kind: CaseKind;
  status: CaseStatus;
  customer: string;
  contact: string;
  subject: string;
  messages: CaseMsg[];
  assignee?: string;
  ticketId?: string;
  rating?: number;
  token: string;
  createdAt: number;
  updatedAt: number;
}

export interface Deal {
  id: string;
  projectId: string;
  customer: string;
  plan: string;
  amountVnd: number;
  employeeId: string;
  ts: number;
}

export interface Settings {
  autoApproveCommands: boolean;
  maxConcurrent: number;
  maxBounces: number;
  maxAgentSteps: number;
  defaultBudgetUsd: number;
  mockDelayMs: number;
}

export interface CompanyState {
  employees: Employee[];
  runtime: Record<string, EmpRuntime>;
  projects: Project[];
  tickets: Ticket[];
  messages: ChatMsg[];
  approvals: Approval[];
  activity: Activity[];
  usage: Record<string, UsageStat>;
  kpi: Record<string, Kpi>;
  cases: SupportCase[];
  deals: Deal[];
  settings: Settings;
}

export type Coll = "employees" | "projects" | "tickets" | "messages" | "approvals" | "activity" | "cases" | "deals";

export type ServerEvent =
  | { type: "state"; state: CompanyState; providers: Record<ProviderId, boolean> }
  | { type: "upsert"; coll: Coll; item: any }
  | { type: "remove"; coll: Coll; id: string }
  | { type: "runtime"; id: string; runtime: EmpRuntime }
  | { type: "usage"; id: string; usage: UsageStat }
  | { type: "kpi"; id: string; kpi: Kpi }
  | { type: "settings"; settings: Settings }
  | { type: "handoff"; from: string; to: string; text: string }
  | { type: "meeting"; projectId: string; participants: string[]; topic: string; active: boolean };
