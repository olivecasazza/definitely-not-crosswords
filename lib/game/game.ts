import type { Question } from '@prisma/client'
import type { ICoordinates } from './boardState'
import type { WithComputedProperties } from './question'

export function GetActiveGameCellFromCoordinates (
  cordX: number,
  cordY: number,
  questions: WithComputedProperties<Question>[]
) {
  const answer = questions
    .flatMap(q => q.answerMap)
    .find(a => a.cordX === cordX && a.cordY === cordY)
  if (!answer) { throw new Error('could not find cell matching coordinates') }
  return answer
}

export function GetGameBoardSize (
  questions: WithComputedProperties<Question>[]
): ICoordinates {
  const boardSize = questions
    .flatMap(q => q.answerMap)
    .reduce(
      (pre, cur) => {
        const largest = pre
        if (cur.cordX > pre.x) { largest.x = cur.cordX }
        if (cur.cordY > pre.y) { largest.y = cur.cordY }
        return largest
      },
      { x: 0, y: 0 }
    )
  boardSize.x++
  boardSize.y++
  return boardSize
}
