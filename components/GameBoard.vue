<script setup lang="ts">
import { storeToRefs } from 'pinia';
import { Cell } from '~/lib/game';
import { useActiveGameStore } from '~/stores/activeGame';
import { computed } from 'vue';

const activeGameStore = useActiveGameStore()
const { boardState, selectedQuestion, gameActionData, boardSize, questions, focusedIndex } = storeToRefs(activeGameStore)
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

// Flat-mapped cells for single CSS Grid
const flatCells = computed(() => {
  return boardState.value.flat();
});

// Dynamic aspect ratio based on rows and columns
const boardAspectRatio = computed(() => {
  if (!boardSize.value.x || !boardSize.value.y) return 1;
  return boardSize.value.x / boardSize.value.y;
});

// Get starting clue number for standard crossword layout
function getCellNumber(cell: Cell): number | null {
  if (cell.cordX === -1 || cell.cordY === -1) return null;
  const startQ = questions.value.find(q => q.rootX === cell.cordX && q.rootY === cell.cordY);
  return startQ ? startQ.number : null;
}

// Check if this is the active cell currently being focused
function isFocusedCell(cell: Cell): boolean {
  if (!selectedQuestion.value || focusedIndex.value === null) return false;
  const activeCell = selectedQuestion.value.answerMap[focusedIndex.value];
  return activeCell && activeCell.cordX === cell.cordX && activeCell.cordY === cell.cordY;
}
</script>

<template>
  <div class="w-full max-w-xl flex justify-center p-3 sm:p-5 bg-[var(--bg-card)] border border-[var(--border-app)] rounded-2xl shadow-sm transition-all duration-300">
    <div 
      class="grid gap-[2px] sm:gap-[4px] w-full max-w-[min(90vw,480px)]"
      :style="{
        gridTemplateColumns: `repeat(${boardSize.x}, minmax(0, 1fr))`,
        gridTemplateRows: `repeat(${boardSize.y}, minmax(0, 1fr))`,
        aspectRatio: boardAspectRatio
      }"
    >
      <div 
        v-for="cell of flatCells" 
        :key="`${cell.cordX}-${cell.cordY}`" 
        @click="isLetter(cell) && selectCoordinates(cell.cordX, cell.cordY)" 
        :class="[
          'relative aspect-square w-full select-none rounded transition-all duration-150',
          isLetter(cell) ? 'cursor-pointer hover:scale-[1.02] active:scale-[0.98]' : 'pointer-events-none'
        ]"
      >
        <!-- Empty crossword cell -->
        <div v-if="!isLetter(cell)" class="w-full h-full bg-[var(--bg-cell-empty)] border border-[rgba(39,39,42,0.25)] opacity-40 rounded"></div>
        
        <!-- Letter cell -->
        <div v-else class="w-full h-full">
          <div :class="[
            'w-full h-full rounded border flex items-center justify-center font-mono font-bold text-base sm:text-lg md:text-xl transition-all duration-150 relative uppercase select-none',
            isFocusedCell(cell) 
              ? 'bg-[var(--pastel-yellow)] text-slate-900 border-[var(--pastel-yellow)] scale-105 shadow-[0_0_12px_rgba(254,234,153,0.35)] z-10' 
              : '',
            isSelected(cell) && !isFocusedCell(cell) 
              ? 'bg-[rgba(254,234,153,0.18)] text-[var(--text-primary)] border-[var(--pastel-yellow)]' 
              : '',
            !isSelected(cell) && !cell.modifications?.length 
              ? 'bg-[var(--bg-cell-letter)] text-[var(--text-primary)] border-[var(--border-app)] hover:border-[var(--border-hover)]' 
              : '',
            !isSelected(cell) && cell.modifications?.length && cell.modifications[0].actionType === 'placeholder' 
              ? 'bg-[var(--bg-cell-letter)] text-[var(--text-primary)] border-[var(--pastel-yellow)] border-2' 
              : '',
            !isSelected(cell) && cell.modifications?.length && cell.modifications[0].actionType === 'incorrectGuess' 
              ? 'bg-[rgba(255,140,140,0.15)] text-[var(--pastel-red)] border-[var(--pastel-red)] font-semibold' 
              : '',
            !isSelected(cell) && cell.modifications?.length && cell.modifications[0].actionType === 'correctGuess' 
              ? 'bg-[rgba(168,230,207,0.15)] text-[var(--pastel-green)] border-[var(--pastel-green)] font-semibold' 
              : ''
          ]">
            <!-- Tiny starting number -->
            <span v-if="getCellNumber(cell)" class="absolute top-[2px] left-[3px] text-[7px] sm:text-[9px] font-mono leading-none text-[var(--text-secondary)] opacity-85 pointer-events-none select-none font-bold">
              {{ getCellNumber(cell) }}
            </span>
            
            <!-- Letter Character -->
            <span>
              {{ isSelected(cell) ? getTypedState(cell) : (cell.modifications?.length ? cell.modifications[0].state : '') }}
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Scoped styles kept minimal due to robust Tailwind integration */
</style>
