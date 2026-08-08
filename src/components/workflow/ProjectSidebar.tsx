import { Archive, FolderKanban, Pencil, Plus, X } from "lucide-react";
import type { WorkLedgerProject, WorkLedgerTask } from "../../lib/desktop";

export function ProjectSidebar({
  projects,
  tasks,
  selectedProjectId,
  drawerOpen = false,
  hidden = false,
  pendingArchiveProjectId,
  onSelect,
  onCreate,
  onEdit,
  onArchive,
  onClose,
}: {
  projects: WorkLedgerProject[];
  tasks: WorkLedgerTask[];
  selectedProjectId: string | null;
  drawerOpen?: boolean;
  hidden?: boolean;
  pendingArchiveProjectId?: string;
  onSelect: (projectId: string) => void;
  onCreate: () => void;
  onEdit: (project: WorkLedgerProject) => void;
  onArchive: (project: WorkLedgerProject) => void;
  onClose?: () => void;
}) {
  return <aside
    id="workflow-project-drawer"
    className={`workflow-project-rail ${drawerOpen ? "drawer-open" : ""}`}
    aria-label="项目列表"
    hidden={hidden}
    role={drawerOpen ? "dialog" : undefined}
    aria-modal={drawerOpen ? true : undefined}
    tabIndex={drawerOpen ? -1 : undefined}
    onKeyDown={(event) => event.key === "Escape" && onClose?.()}
  >
    <header className="workflow-area-header">
      <div><FolderKanban size={16} /><strong>项目</strong><span>{projects.length}</span></div>
      <div className="workflow-icon-actions">
        <button type="button" className="workflow-icon-button workflow-drawer-close" aria-label="关闭项目抽屉" title="关闭项目" onClick={onClose}><X size={16} /></button>
        <button type="button" className="workflow-icon-button" aria-label="新建项目" title="新建项目" onClick={onCreate}><Plus size={16} /></button>
      </div>
    </header>
    <div className="workflow-project-list">
      {projects.map((project) => {
        const selected = project.id === selectedProjectId;
        const projectTasks = tasks.filter((task) => task.projectId === project.id);
        const completed = projectTasks.filter((task) => task.status === "completed").length;
        return <div className={`workflow-project-item ${selected ? "selected" : ""}`} key={project.id}>
          <button type="button" className="workflow-project-select" aria-current={selected ? "page" : undefined} onClick={() => onSelect(project.id)}>
            <i style={{ background: project.color }} />
            <span><strong>{project.name}</strong><small>{completed}/{projectTasks.length} 已完成</small></span>
          </button>
          {selected && <div className="workflow-row-actions">
            <button type="button" aria-label="编辑项目" title="编辑项目" disabled={pendingArchiveProjectId === project.id} onClick={() => onEdit(project)}><Pencil size={14} /></button>
            <button type="button" aria-label="归档项目" title="归档项目" disabled={pendingArchiveProjectId === project.id} onClick={() => onArchive(project)}><Archive size={14} /></button>
          </div>}
        </div>;
      })}
      {!projects.length && <p className="workflow-empty">暂无项目</p>}
    </div>
  </aside>;
}
