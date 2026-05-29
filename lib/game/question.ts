import type { GameAction, Question } from '@prisma/client'
import { ICoordinates } from './boardState'
import { SortGameActionByDateDesc } from './gameAction'
import type { Cell } from './cell'

// Extend the T generic with the fullName attribute
export type WithComputedProperties<Question> = Question & {
  answerMap: Cell[];
};

// Take objects that satisfy FirstLastName and computes a full name
export function computeQuestionAnswerMap<T extends Question> (
  dbo: T,
  actions: GameAction[]
): WithComputedProperties<T> {
  let answerMap: Cell[] = new Array(dbo.answer.length)
  // default state
  dbo.answer.split('').forEach((character, index) => {
    let x = dbo.rootX
    let y = dbo.rootY
    dbo.direction === "ACROSS"
      ? (x += index)
      : (y += index)
    answerMap[index] = {
      cordX: x,
      cordY: y,
      correctState: character,
      modifications: []
    }
  })
  // put each action into the correct cells modifications
  actions.forEach((action) => {
    const answerMapIndex = answerMap.findIndex(cell =>
      ICoordinates.IsEqual(
        { x: cell.cordX, y: cell.cordY },
        {
          x: action.cordX,
          y: action.cordY
        }
      )
    )

    if (answerMapIndex !== -1) {
      answerMap[answerMapIndex].modifications.push(action)
    }
  })
  // make sure modifications for each cell are sorted
  answerMap = answerMap.map((cell) => {
    cell.modifications = cell.modifications.sort(SortGameActionByDateDesc)
    return cell
  })
  return { ...dbo, answerMap }
}

export const GetCordAtAnswerIndex = (
  q: Question,
  index: number
): ICoordinates => {
  let cordX = q.rootX
  let cordY = q.rootY
  q.direction === "ACROSS"
    ? (cordX += index)
    : (cordY += index)
  return { x: cordX, y: cordY }
}
