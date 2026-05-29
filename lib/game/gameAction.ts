import type { GameAction } from "@prisma/client";

export const SortGameActionByDateDesc = (a: GameAction, b: GameAction) =>
  a.submittedAt > b.submittedAt ? -1 : a.submittedAt <= b.submittedAt ? 1 : 0;
