import type { Question } from '@prisma/client'
import type { ICoordinates } from './boardState'
import type { WithComputedProperties } from './question'

export function GetBoardSize (questions: WithComputedProperties<Question>[]): ICoordinates {
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
