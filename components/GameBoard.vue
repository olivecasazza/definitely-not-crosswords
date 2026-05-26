<script setup lang="ts">
import { storeToRefs } from 'pinia';
import { Cell } from '~/lib/game';
import { useActiveGameStore } from '~/stores/activeGame';

const activeGameStore = useActiveGameStore()
const { boardState, selectedQuestion, gameActionData } = storeToRefs(activeGameStore)
const { selectCoordinates } = activeGameStore

function isLetter(cell: Cell): boolean {
  return cell.correctState !== ''
}

function isSelected(cell: Cell): boolean {
  if (!selectedQuestion.value) return false
  if (!selectedQuestion.value?.answerMap) return false;
  return selectedQuestion.value.answerMap.some(c => c.cordX === cell.cordX && c.cordY === cell.cordY)
}

function getTypedState(cell: Cell): string {
  if (!selectedQuestion.value || !gameActionData.value) return ''
  const typedCell = gameActionData.value.find(c => c.cordX === cell.cordX && c.cordY === cell.cordY)
  return typedCell ? typedCell.state : ''
}
</script>

<template>
  <div class="flex flex-col gap-1 items-center justify-center p-6 app-card max-w-fit mx-auto">
    <div v-for="(cellRow, rowIndex) of boardState" :key="rowIndex" class="flex flex-row gap-1">
      <div v-for="cell of cellRow" :key="cell.cordX" @click="selectCoordinates(cell.cordX, cell.cordY)" class="cursor-pointer">
        <div v-if="!isLetter(cell)" class="cell empty rounded"></div>
        <div v-else>
          <div :class="[
            'cell letter rounded font-mono font-bold text-lg flex items-center justify-center transition-all duration-150 select-none border',
            isSelected(cell) ? 'bg-[var(--pastel-yellow)] text-slate-900 border-[var(--pastel-yellow)] scale-105 shadow-sm' : '',
            !cell.modifications?.length && !isSelected(cell) ? 'bg-[var(--bg-cell-letter)] text-[var(--text-primary)] border-[var(--border-app)] hover:border-[var(--border-hover)]' : '',
            cell.modifications?.length && cell.modifications[0].actionType === 'placeholder' && !isSelected(cell) ? 'bg-[var(--bg-cell-letter)] text-[var(--text-primary)] border-[var(--pastel-yellow)] border-2' : '',
            cell.modifications?.length && cell.modifications[0].actionType === 'incorrectGuess' && !isSelected(cell) ? 'bg-[var(--pastel-red)] text-slate-900 border-[var(--pastel-red)]' : '',
            cell.modifications?.length && cell.modifications[0].actionType === 'correctGuess' && !isSelected(cell) ? 'bg-[var(--pastel-green)] text-slate-900 border-[var(--pastel-green)]' : '',
          ]">
            {{ isSelected(cell) ? getTypedState(cell) : (cell.modifications?.length ? cell.modifications[0].state : '') }}
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cell {
  width: 2.25rem;
  height: 2.25rem;
  text-align: center;
}

.empty {
  background-color: var(--bg-cell-empty);
  border: 1px solid var(--border-app);
  opacity: 0.4;
}

.letter {
  text-transform: uppercase;
}
</style>
