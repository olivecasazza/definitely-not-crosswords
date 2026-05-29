import type { GameAction } from '@prisma/client';

export type CellState = string | '';
export interface Cell {
  modifications: GameAction[];
  correctState: CellState;
  cordX: number,
  cordY: number
}

export function GetCurrentCellState (cell: Cell): CellState {
  if (!cell?.modifications[0]?.state) { return '' }
  return cell.modifications[0].state
}

export function IsCellCorrect (cell: Cell): boolean {
  if (cell.correctState == '') { return false }
  if (cell.modifications.length == 0) { return false }
  const latestModification = GetCurrentCellState(cell)
  if (latestModification == '') { return false }
  return latestModification === cell.correctState
}
