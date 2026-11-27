import { useStore } from "@/store";
import { Avatar } from "@/shared/Avatar";
import { ROLE_LABEL } from "@/shared/labels";
import type { Employee } from "@/api/types";

const BUSY = ["working", "thinking", "waiting", "meeting"];

/** People shown as avatars: a green dot when working; a click opens their screen and chat. */
export function PeopleRow({ people, label }: { people: Employee[]; label: string }) {
  const { state, setWatching } = useStore();
  if (!people.length) return null;
  return (
    <div className="team-row" onClick={(e) => e.stopPropagation()}>
      <span className="small muted">{label}</span>
      <div className="team-faces">
        {people.map((e) => {
          const rt = state?.runtime[e.id];
          const busy = BUSY.includes(rt?.status ?? "idle");
          return (
            <button key={e.id} className={`face${busy ? " busy" : ""}`} onClick={() => setWatching(e.id)} title={`${e.name} · ${ROLE_LABEL[e.role]} — ${busy ? rt?.note || "đang làm" : "đang rảnh"} (bấm để xem màn hình / nhắn việc)`}>
              <Avatar e={e} small />
              <span className="face-name">{e.name}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** The project's team (picked when it started). */
export function TeamRow({ team }: { team: string[] }) {
  const { state } = useStore();
  const people = team.map((id) => state?.employees.find((e) => e.id === id)).filter((e): e is Employee => !!e && e.enabled);
  return <PeopleRow people={people} label={`Nhóm dự án (${people.length})`} />;
}

/** Everyone with nothing to do right now: the chairman can give them a task in chat. */
export function IdleRow() {
  const { state } = useStore();
  if (!state) return null;
  const idle = state.employees.filter((e) => e.enabled && !BUSY.includes(state.runtime[e.id]?.status ?? "idle"));
  return <PeopleRow people={idle} label={`Đang rảnh (${idle.length}) — bấm để nhắn giao việc`} />;
}
