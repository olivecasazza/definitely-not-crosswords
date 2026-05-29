import type { GameAction, Question } from "@prisma/client";
import type { Cell } from "./cell";
import type { WithComputedProperties } from "./question";
import { SortGameActionByDateDesc } from "./gameAction";

export class ICoordinates {
  x: number = 0;
  y: number = 0;

  public static IsEqual(a: ICoordinates, b: ICoordinates): boolean {
    return a.x === b.x && a.y == b.y;
  }
}
export interface IBoardState extends Array<Array<Cell>> {}

export class BoardState {
  static BoardStateFromActions(
    boardSize: ICoordinates,
    actions: GameAction[],
    questions: WithComputedProperties<Question>[]
  ): IBoardState {
    console.log("COMPUTING BOARD STATE");
    console.log(actions);
    // console.log(questions);

    const answersCellMap = questions.flatMap((q) => [...q.answerMap]);
    const boardState = new Array(boardSize.y);
    for (let yi = 0; yi < boardSize.y; yi++) {
      boardState[yi] = new Array(boardSize.x);
      for (let xi = 0; xi < boardSize.x; xi++) {
        const coordinates = { x: xi, y: yi };
        const answerCell = answersCellMap.find((cell) =>
          ICoordinates.IsEqual({ x: cell.cordX, y: cell.cordY }, coordinates)
        );
        let boardCell: Cell;
        if (!answerCell)
          boardCell = {
            cordX: -1,
            cordY: -1,
            correctState: "",
            modifications: [],
          } as Cell;
        else {
          boardCell = { ...answerCell, modifications: [] };
          boardCell.modifications = actions
            .filter(
              (action) =>
                action.cordX === boardCell.cordX &&
                action.cordY === boardCell.cordY
            )
            .sort(SortGameActionByDateDesc);
        }
        boardState[yi][xi] = boardCell;
      }
    }
    console.log(boardState);
    return boardState;
  }
}
