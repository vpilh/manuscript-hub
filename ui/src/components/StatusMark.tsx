import {
  CircleCheck,
  CircleX,
  FileCheck2,
  Hourglass,
  PenLine,
  Send,
  Undo2,
  UserRound,
  type LucideIcon,
} from "lucide-react";

import type { EditorialStatus } from "../lib/api";
import { statusLabel } from "../lib/format";

const ICONS: Record<EditorialStatus, LucideIcon> = {
  submitted: Send,
  with_editor: UserRound,
  under_review: Hourglass,
  revision_required: PenLine,
  revised: FileCheck2,
  accepted: CircleCheck,
  rejected: CircleX,
  withdrawn: Undo2,
};

/** Tone drives color only; the word and icon carry the meaning. */
const TONES: Partial<Record<EditorialStatus, string>> = {
  accepted: "positive",
  revision_required: "caution",
};

export function StatusMark({ status }: { status: EditorialStatus | null }) {
  if (!status) {
    return <span className="status-mark none">No status yet</span>;
  }
  const Icon = ICONS[status];
  return (
    <span className={`status-mark ${TONES[status] ?? ""}`}>
      <Icon aria-hidden="true" size={16} strokeWidth={2} />
      {statusLabel(status)}
    </span>
  );
}
