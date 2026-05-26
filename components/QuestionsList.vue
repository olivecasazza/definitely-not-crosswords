<script setup lang="ts">
import { Question } from "@prisma/client";
import { storeToRefs } from "pinia";
import { useActiveGameStore } from "~/stores/activeGame";
import { OnClickOutside } from '@vueuse/components'

const activeGameStore = useActiveGameStore();
const { submitActions, selectQuestion, unSelect } = activeGameStore;
const { selectedQuestion, filteredQuestions, gameActionData } = storeToRefs(activeGameStore);

function isSelected(question: Question): boolean {
  if (!selectedQuestion?.value) return false;
  return selectedQuestion.value.id === question.id;
}

function keyup(e: KeyboardEvent) {
  if ((e.keyCode >= 48 && e.keyCode <= 57) || (e.keyCode >= 65 && e.keyCode <= 90)) {
    const nextInput = (e.target as HTMLElement)?.nextElementSibling as HTMLInputElement | null;
    if (nextInput && nextInput.tagName === 'INPUT') {
      nextInput.focus();
      nextInput.select();
    }
  } else if (e.key === 'Backspace') {
    const prevInput = (e.target as HTMLElement)?.previousElementSibling as HTMLInputElement | null;
    if (prevInput && prevInput.tagName === 'INPUT') {
      prevInput.focus();
      prevInput.select();
    }
  }
}
</script>

<template>
  <div class="flex flex-col gap-3 p-4 app-card max-h-[400px] overflow-y-auto w-full max-w-xl mx-auto">
    <h2 class="text-[var(--text-secondary)] font-semibold text-xs tracking-wider uppercase px-1">Clues</h2>
    <div class="flex flex-col gap-2">
      <div v-for="question in filteredQuestions" :key="question.id" :ref="question.id" @click="selectQuestion(question)">
        <div :class="[
          'flex flex-row gap-3 p-3 rounded border transition-all duration-150 cursor-pointer',
          isSelected(question) ? 'bg-[var(--bg-cell-empty)] border-[var(--pastel-yellow)]' : 'bg-transparent border-[var(--border-app)] hover:border-[var(--border-hover)]'
        ]">
          <div :class="[
            'w-8 h-8 rounded flex items-center justify-center font-mono font-bold text-sm border shrink-0',
            isSelected(question) ? 'bg-[var(--pastel-yellow)] text-slate-900 border-[var(--pastel-yellow)]' : 'bg-[var(--bg-cell-empty)] text-[var(--text-secondary)] border-[var(--border-app)]'
          ]">
            {{ question.number }}
          </div>
          
          <div class="flex flex-col w-full gap-2">
            <div :class="[
              'text-sm font-medium leading-relaxed',
              isSelected(question) ? 'text-[var(--text-primary)]' : 'text-[var(--text-secondary)]'
            ]">
              {{ question.questionText }}
            </div>
            
            <div v-if="isSelected(question)" class="flex flex-col gap-2 mt-1" @click.stop>
              <OnClickOutside @trigger="unSelect" class="flex flex-wrap items-center gap-1.5">
                <input v-for="modification of gameActionData" 
                  :key="modification.cordX + '-' + modification.cordY"
                  class="w-9 h-9 app-input text-center text-lg font-bold uppercase font-mono" 
                  v-model="modification.state"
                  @input="modification.state = modification.state.toUpperCase()"
                  type="text" maxlength="1" @keyup="keyup" />
                
                <button class="app-btn app-btn-active ml-2" @click="submitActions('guess', question)">
                  Guess
                </button>
              </OnClickOutside>
            </div>
            
            <div v-else class="flex flex-row gap-1">
              <div v-for="cell in question.answerMap" :key="cell.cordX + '-' + cell.cordY">
                <div v-if="!cell.modifications?.length" class="w-5 h-5 rounded bg-[var(--bg-cell-empty)] border border-[var(--border-app)] opacity-30"></div>
                <div v-else-if="cell.modifications[0].actionType === 'placeholder'" class="w-5 h-5 rounded flex items-center justify-center font-mono font-bold text-xxs bg-[var(--bg-cell-letter)] text-[var(--text-primary)] border border-[var(--pastel-yellow)] uppercase">{{ cell?.modifications[0].state }}</div>
                <div v-else-if="cell.modifications[0].actionType === 'incorrectGuess'" class="w-5 h-5 rounded flex items-center justify-center font-mono font-bold text-xxs bg-[var(--pastel-red)] text-slate-900 uppercase">{{ cell?.modifications[0].state }}</div>
                <div v-else-if="cell.modifications[0].actionType === 'correctGuess'" class="w-5 h-5 rounded flex items-center justify-center font-mono font-bold text-xxs bg-[var(--pastel-green)] text-slate-900 uppercase">{{ cell?.modifications[0].state }}</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.text-xxs {
  font-size: 0.65rem;
}
</style>
