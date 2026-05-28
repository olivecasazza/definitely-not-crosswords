<script setup lang="ts">
import { storeToRefs } from 'pinia';
import { useActiveGameStore } from '~/stores/activeGame';
import { OnClickOutside } from '@vueuse/components';
import { reactive, nextTick, watch } from 'vue';

const activeGameStore = useActiveGameStore();
const { selectedQuestion, gameActionData, focusedIndex } = storeToRefs(activeGameStore);
const { submitActions, unSelect } = activeGameStore;

const inputs = reactive<Record<number, HTMLInputElement>>({});

function focusIndex(idx: number) {
  if (idx >= 0 && idx < gameActionData.value.length) {
    focusedIndex.value = idx;
    nextTick(() => {
      const el = inputs[idx];
      if (el) {
        el.focus();
        el.select();
      }
    });
  }
}

function handleInput(e: Event, index: number) {
  const inputEl = e.target as HTMLInputElement;
  const val = inputEl.value.toUpperCase();
  gameActionData.value[index].state = val;
  
  if (val && index < gameActionData.value.length - 1) {
    focusIndex(index + 1);
  }
}

function handleKeyDown(e: KeyboardEvent, index: number) {
  if (e.key === 'Backspace') {
    if (!gameActionData.value[index].state && index > 0) {
      gameActionData.value[index - 1].state = '';
      focusIndex(index - 1);
    } else {
      gameActionData.value[index].state = '';
    }
    e.preventDefault();
  } else if (e.key === 'ArrowLeft' && index > 0) {
    focusIndex(index - 1);
    e.preventDefault();
  } else if (e.key === 'ArrowRight' && index < gameActionData.value.length - 1) {
    focusIndex(index + 1);
    e.preventDefault();
  }
}

// Auto-focus the first input whenever the active question changes
watch(selectedQuestion, (newQuestion) => {
  if (newQuestion) {
    // Clear old inputs map to avoid memory leak / mismatch
    for (const key in inputs) {
      delete inputs[key];
    }
    nextTick(() => {
      focusIndex(0);
    });
  }
});
</script>

<template>
  <div class="w-full max-w-xl transition-all duration-300">
    <div v-if="selectedQuestion" class="app-card p-5 sm:p-6 flex flex-col gap-4 relative overflow-hidden bg-gradient-to-br from-[var(--bg-card)] to-[var(--bg-app)] shadow-md border-[var(--border-app)]">
      <!-- Glow effect decorative corner -->
      <div class="absolute top-0 right-0 w-24 h-24 bg-[var(--pastel-yellow)] opacity-[0.03] rounded-full blur-2xl pointer-events-none"></div>
      
      <!-- Clue Meta Information Header -->
      <div class="flex flex-row justify-between items-center border-b border-[var(--border-app)] pb-3">
        <div class="flex items-center gap-2">
          <span :class="[
            'text-[10px] font-sans tracking-widest font-bold px-2 py-0.5 rounded uppercase border',
            selectedQuestion.direction === 'ACROSS' 
              ? 'bg-[rgba(254,234,153,0.1)] text-[var(--pastel-yellow)] border-[rgba(254,234,153,0.2)]'
              : 'bg-[rgba(168,230,207,0.1)] text-[var(--pastel-green)] border-[rgba(168,230,207,0.2)]'
          ]">
            {{ selectedQuestion.direction }}
          </span>
          <span class="text-xs font-sans font-semibold text-[var(--text-secondary)] tracking-wider">
            CLUE {{ selectedQuestion.number }} &bull; {{ selectedQuestion.answer.length }} LETTERS
          </span>
        </div>
        <button @click="unSelect" class="text-xs font-sans text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors">
          ESC to clear
        </button>
      </div>

      <!-- Clue Text -->
      <div class="text-base sm:text-lg font-medium leading-relaxed text-[var(--text-primary)] px-1">
        {{ selectedQuestion.questionText }}
      </div>

      <!-- Answer Character Inputs Row -->
      <OnClickOutside @trigger="unSelect" class="flex flex-col gap-4 mt-1">
        <div class="flex flex-wrap items-center justify-center gap-2 py-2">
          <input 
            v-for="(modification, index) of gameActionData" 
            :key="modification.cordX + '-' + modification.cordY"
            :ref="el => { if (el) inputs[index] = el as HTMLInputElement }"
            class="w-9 h-9 sm:w-11 sm:h-11 app-input text-center text-lg sm:text-xl font-bold uppercase font-sans transition-all duration-150 focus:scale-105"
            :class="[
              focusedIndex === index 
                ? 'border-[var(--pastel-yellow)] shadow-[0_0_12px_rgba(254,234,153,0.25)] ring-1 ring-[var(--pastel-yellow)]' 
                : 'border-[var(--border-app)] hover:border-[var(--border-hover)]'
            ]"
            v-model="modification.state"
            @input="handleInput($event, index)"
            @focus="focusedIndex = index"
            @keydown="handleKeyDown($event, index)"
            type="text" 
            maxlength="1" 
            autocomplete="off"
            spellcheck="false"
          />
        </div>
        
        <!-- Guess Action Buttons Row -->
        <div class="flex flex-row justify-end gap-3 mt-1 px-1 select-none">
          <button 
            class="px-4 py-2 text-xs font-bold uppercase tracking-wider rounded-lg border border-[var(--border-app)] bg-[var(--bg-card)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--border-hover)] hover:scale-[1.02] active:scale-[0.98] transition-all duration-150 cursor-pointer font-sans" 
            @click="unSelect"
          >
            Cancel
          </button>
          <button 
            class="px-5 py-2 text-xs font-bold uppercase tracking-wider rounded-lg border border-[var(--pastel-yellow)] bg-gradient-to-r from-[var(--pastel-yellow)] to-[rgba(254,234,153,0.85)] text-slate-900 shadow-md hover:shadow-lg hover:scale-[1.03] active:scale-[0.97] transition-all duration-150 cursor-pointer font-sans" 
            @click="submitActions('guess', selectedQuestion)"
          >
            Guess
          </button>
        </div>
      </OnClickOutside>
    </div>
    
    <!-- Empty State -->
    <div v-else class="app-card p-6 sm:p-8 flex flex-col items-center justify-center text-center border-dashed border-2 border-[var(--border-app)] bg-[rgba(24,24,27,0.2)]">
      <div class="w-10 h-10 rounded-full bg-[rgba(27,27,30,0.4)] border border-[var(--border-app)] flex items-center justify-center mb-3">
        <svg class="w-5 h-5 text-[var(--text-secondary)] opacity-60" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
        </svg>
      </div>
      <h3 class="text-base font-semibold text-[var(--text-primary)] font-serif">Ready to solve?</h3>
      <p class="text-xs text-[var(--text-secondary)] mt-1.5 max-w-xs leading-relaxed">
        Tap a square on the crossword board or select a clue from the list to start typing your answers.
      </p>
    </div>
  </div>
</template>

<style scoped>
.focus\:scale-105:focus {
  transform: scale(1.05);
}
</style>
