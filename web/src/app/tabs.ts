import type { ComponentType } from "react";
import type { CompanyState } from "@/api/types";
import { ProjectsPanel } from "@/features/projects/ProjectsPanel";
import { OpsPanel } from "@/features/ops/OpsPanel";
import { KanbanPanel } from "@/features/kanban/KanbanPanel";
import { ChatPanel } from "@/features/chat/ChatPanel";
import { StaffPanel } from "@/features/staff/StaffPanel";
import { KpiPanel } from "@/features/kpi/KpiPanel";
import { ProductPanel } from "@/features/product/ProductPanel";
import { ApprovalsPanel } from "@/features/approvals/ApprovalsPanel";
import { CostsPanel } from "@/features/costs/CostsPanel";
import { SettingsPanel } from "@/features/settings/SettingsPanel";

export interface TabDef {
  id: string;
  label: string;
  panel: ComponentType;
  /** Number shown as a red badge on the tab (0 = hidden). */
  badge?: (s: CompanyState) => number;
}

/** Right-hand tabs, in display order. To add a tab: make a panel in features/<name>/ and add one line here. */
export const TABS: TabDef[] = [
  { id: "projects", label: "Dự án", panel: ProjectsPanel },
  {
    id: "ops",
    label: "Vận hành",
    panel: OpsPanel,
    badge: (s) => (s.cases ?? []).filter((c) => c.status === "open" || c.status === "in_progress").length,
  },
  { id: "kanban", label: "Kanban", panel: KanbanPanel },
  { id: "chat", label: "Chat", panel: ChatPanel },
  { id: "staff", label: "Nhân sự", panel: StaffPanel },
  { id: "kpi", label: "KPI", panel: KpiPanel },
  { id: "product", label: "Sản phẩm", panel: ProductPanel },
  { id: "approvals", label: "Duyệt", panel: ApprovalsPanel, badge: (s) => s.approvals.filter((a) => a.status === "pending").length },
  { id: "costs", label: "Chi phí", panel: CostsPanel },
  { id: "settings", label: "Cài đặt", panel: SettingsPanel },
];
