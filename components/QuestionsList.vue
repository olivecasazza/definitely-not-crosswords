<script setup lang="ts">
import { Question } from "@prisma/client";
import { storeToRefs } from "pinia";
import { useActiveGameStore } from "~/stores/activeGame";
import { reactive, watch, nextTick } from "vue";
import { Cell } from "~/lib/game";

const activeGameStore = useActiveGameStore();
const { selectQuestion, filterDown, filterAcross } = activeGameStore;
const { selectedQuestion, filteredQuestions, gameActionData, selectedDirection } = storeToRefs(activeGameStore);

const clueElements = reactive<Record<string, HTMLElement>>({});

function isSelected(question: Question): boolean {
  if (!selectedQuestion?.value) return false;
  return selectedQuestion.value.id === question.id;
}

// Helper to determine the cell display state in real-time
function getCellDisplayState(question: Question, cell: Cell): { state: string, actionType: string } {
  // If this is the active selected question, pull live from gameActionData!
  if (selectedQuestion.value?.id === question.id && gameActionData.value.length) {
    const match = gameActionData.value.find(c => c.cordX === cell.cordX && c.cordY === cell.cordY);
    if (match) {
      return {
        state: match.state,
        actionType: match.actionType || 'placeholder'
      };
    }
  }
  
  // Otherwise, use the cell's saved modifications
  if (cell.modifications?.length) {
    return {
      state: cell.modifications[0].state,
      actionType: cell.modifications[0].actionType
    };
  }
  
  return { state: '', actionType: '' };
}

// Auto-scroll selected question into view
watch(selectedQuestion, (newQuestion) => {
  if (newQuestion) {
    nextTick(() => {
      const el = clueElements[newQuestion.id];
      if (el) {
        el.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      }
    });
  }
});
</script>

<template>
  <div class="flex flex-col gap-3 p-4 sm:p-5 app-card h-full lg:max-h-[calc(100vh-140px)] overflow-hidden w-full max-w-xl mx-auto shadow-sm bg-gradient-to-br from-[var(--bg-card)] to-[var(--bg-app)] border-[var(--border-app)]">
    
    <!-- Consolidated Header Tabs -->
    <div class="flex flex-row justify-between items-center border-b border-[var(--border-app)] pb-3 mb-1 shrink-0">
      <h2 class="text-[var(--text-secondary)] font-semibold text-xs tracking-wider uppercase px-1 font-mono">Clues</h2>
      <div class="flex flex-row gap-1">
        <button 
          :class="[
            'px-3 py-1 font-mono text-[10px] uppercase tracking-wider rounded border transition-all duration-150',
            selectedDirection === 'ACROSS' 
              ? 'bg-[var(--pastel-yellow)] text-slate-900 border-[var(--pastel-yellow)] font-bold'
              : 'bg-transparent text-[var(--text-secondary)] border-[var(--border-app)] hover:border-[var(--border-hover)]'
          ]"
          @click="filterAcross"
        >
          Across
        </button>
        <button 
          :class="[
            'px-3 py-1 font-mono text-[10px] uppercase tracking-wider rounded border transition-all duration-150',
            selectedDirection === 'DOWN' 
              ? 'bg-[var(--pastel-green)] text-slate-900 border-[var(--pastel-green)] font-bold'
              : 'bg-transparent text-[var(--text-secondary)] border-[var(--border-app)] hover:border-[var(--border-hover)]'
          ]"
          @click="filterDown"
        >
          Down
        </button>
      </div>
    </div>
    
    <!-- Clues list with dynamic scroll container -->
    <div class="flex-1 overflow-y-auto pr-1 flex flex-col gap-2.5">
      <div 
        v-for="question in filteredQuestions" 
        :key="question.id" 
        :ref="el => { if (el) clueElements[question.id] = el as HTMLElement }" 
        @click="selectQuestion(question)"
        class="transition-all duration-200"
      >
        <div :class="[
          'flex flex-row gap-3 p-3 rounded-xl border transition-all duration-150 cursor-pointer relative overflow-hidden',
          isSelected(question) 
            ? 'bg-[rgba(254,234,153,0.03)] border-[rgba(254,234,153,0.4)] shadow-[0_2px_8px_rgba(0,0,0,0.2)]' 
            : 'bg-transparent border-[var(--border-app)] hover:border-[var(--border-hover)]'
        ]">
          <!-- Glow indicator for selected clue -->
          <div v-if="isSelected(question)" class="absolute left-0 top-0 bottom-0 w-[3px] bg-[var(--pastel-yellow)]"></div>
          
          <!-- Clue Number Badge -->
          <div :class="[
            'w-8 h-8 rounded-lg flex items-center justify-center font-mono font-bold text-sm border shrink-0 transition-colors duration-150',
            isSelected(question) 
              ? 'bg-[var(--pastel-yellow)] text-slate-900 border-[var(--pastel-yellow)]' 
              : 'bg-[var(--bg-cell-empty)] text-[var(--text-secondary)] border-[var(--border-app)]'
          ]">
            {{ question.number }}
          </div>
          
          <!-- Clue Text & Guessed State Progress Bar -->
          <div class="flex flex-col w-full gap-2.5">
            <div :class="[
              'text-sm font-medium leading-relaxed transition-colors duration-150',
              isSelected(question) ? 'text-[var(--text-primary)]' : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]'
            ]">
              {{ question.questionText }}
            </div>
            
            <!-- Beautiful Real-time Progress Letter Bubbles -->
            <div class="flex flex-wrap gap-1">
              <div 
                v-for="cell in question.answerMap" 
                :key="cell.cordX + '-' + cell.cordY"
              >
                <!-- Render bubbles using live dynamic state helper -->
                <div v-if="!getCellDisplayState(question, cell).state" class="w-5 h-5 rounded bg-[var(--bg-cell-empty)] border border-[var(--border-app)] opacity-30"></div>
                
                <div v-else-if="getCellDisplayState(question, cell).actionType === 'placeholder'" 
                  class="w-5 h-5 rounded flex items-center justify-center font-mono font-bold text-[10px] bg-[var(--bg-cell-letter)] text-[var(--text-primary)] border border-[var(--pastel-yellow)] uppercase">
                  {{ getCellDisplayState(question, cell).state }}
                </div>
                
                <div v-else-if="getCellDisplayState(question, cell).actionType === 'incorrectGuess'" 
                  class="w-5 h-5 rounded flex items-center justify-center font-mono font-bold text-[10px] bg-[rgba(255,140,140,0.18)] text-[var(--pastel-red)] border border-[var(--pastel-red)] uppercase">
                  {{ getCellDisplayState(question, cell).state }}
                </div>
                
                <div v-else-if="getCellDisplayState(question, cell).actionType === 'correctGuess'" 
                  class="w-5 h-5 rounded flex items-center justify-center font-mono font-bold text-[10px] bg-[rgba(168,230,207,0.18)] text-[var(--pastel-green)] border border-[var(--pastel-green)] uppercase">
                  {{ getCellDisplayState(question, cell).state }}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Scrollbar customized elegantly in tailwind.css, keeping scoped clean */
</style>
