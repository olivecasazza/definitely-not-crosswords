<script setup lang="ts">
type SolvedQuestion = {
  id?: string;
  number: number;
  answer: string;
  questionText: string;
  rootX: number;
  rootY: number;
  direction: "ACROSS" | "DOWN";
};

type SolvedGame = {
  id: string;
  title: string;
  published?: boolean;
  questions: SolvedQuestion[];
};

const props = defineProps<{
  game: SolvedGame;
}>();

const directions = ["ACROSS", "DOWN"] as const;

const sortedQuestions = computed(() =>
  [...props.game.questions].sort((a, b) => a.number - b.number || a.direction.localeCompare(b.direction))
);

const questionsByDirection = computed(() => ({
  ACROSS: sortedQuestions.value.filter((question) => question.direction === "ACROSS"),
  DOWN: sortedQuestions.value.filter((question) => question.direction === "DOWN"),
}));

const gridSize = computed(() => {
  return props.game.questions.reduce(
    (size, question) => {
      const answerLength = question.answer.length;
      return {
        width: Math.max(size.width, question.rootX + (question.direction === "ACROSS" ? answerLength : 1)),
        height: Math.max(size.height, question.rootY + (question.direction === "DOWN" ? answerLength : 1)),
      };
    },
    { width: 0, height: 0 }
  );
});

const letterCells = computed(() => {
  const cells = new Map<string, { letter: string; numbers: number[] }>();

  for (const question of sortedQuestions.value) {
    question.answer.split("").forEach((letter, index) => {
      const x = question.direction === "ACROSS" ? question.rootX + index : question.rootX;
      const y = question.direction === "DOWN" ? question.rootY + index : question.rootY;
      const key = `${x}:${y}`;
      const cell = cells.get(key) ?? { letter: "", numbers: [] };
      cell.letter = letter.toUpperCase();
      if (index === 0 && !cell.numbers.includes(question.number)) {
        cell.numbers.push(question.number);
      }
      cells.set(key, cell);
    });
  }

  return cells;
});

const gridCells = computed(() => {
  const cells: Array<{ key: string; letter: string; numbers: number[]; isBlock: boolean }> = [];

  for (let y = 0; y < gridSize.value.height; y++) {
    for (let x = 0; x < gridSize.value.width; x++) {
      const key = `${x}:${y}`;
      const cell = letterCells.value.get(key);
      cells.push({
        key,
        letter: cell?.letter ?? "",
        numbers: cell?.numbers ?? [],
        isBlock: !cell,
      });
    }
  }

  return cells;
});
</script>

<template>
  <section class="flex flex-col gap-4">
    <div class="flex flex-col gap-1">
      <h3 class="text-sm font-bold uppercase tracking-wider text-[var(--text-primary)]">
        Solved Puzzle
      </h3>
      <p class="text-xs text-[var(--text-secondary)]">
        {{ game.questions.length }} clues in {{ gridSize.width }} x {{ gridSize.height }}
      </p>
    </div>

    <div class="grid grid-cols-1 lg:grid-cols-12 gap-5 items-start">
      <div class="lg:col-span-7 w-full">
        <div
          class="grid w-full max-w-xl mx-auto border border-[var(--border-app)] bg-[var(--border-app)] gap-px"
          :style="{ gridTemplateColumns: `repeat(${gridSize.width}, minmax(0, 1fr))` }"
        >
          <div
            v-for="cell in gridCells"
            :key="cell.key"
            class="relative aspect-square min-w-0 flex items-center justify-center text-sm sm:text-base font-bold"
            :class="cell.isBlock ? 'bg-[var(--bg-cell-empty)]' : 'bg-[var(--bg-cell-letter)] text-[var(--text-primary)]'"
          >
            <span
              v-if="!cell.isBlock && cell.numbers.length"
              class="absolute left-1 top-0.5 text-[7px] sm:text-[8px] leading-none text-[var(--text-secondary)]"
            >
              {{ cell.numbers[0] }}
            </span>
            <span v-if="!cell.isBlock">{{ cell.letter }}</span>
          </div>
        </div>
      </div>

      <div class="lg:col-span-5 grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-1 gap-4">
        <div v-for="direction in directions" :key="direction" class="flex flex-col gap-2">
          <h4 class="text-xs font-mono font-bold uppercase tracking-wider text-[var(--text-secondary)]">
            {{ direction }}
          </h4>
          <ol class="flex flex-col gap-2">
            <li
              v-for="question in questionsByDirection[direction]"
              :key="`${direction}-${question.number}-${question.answer}`"
              class="rounded border border-[var(--border-app)] bg-[var(--bg-cell-empty)] p-3"
            >
              <div class="flex items-start gap-2">
                <span class="min-w-6 text-xs font-bold text-[var(--pastel-yellow)]">{{ question.number }}</span>
                <div class="min-w-0">
                  <p class="text-sm text-[var(--text-primary)]">{{ question.questionText }}</p>
                  <p class="mt-1 text-[10px] font-mono uppercase tracking-wider text-[var(--text-secondary)]">
                    {{ question.answer }}
                  </p>
                </div>
              </div>
            </li>
          </ol>
        </div>
      </div>
    </div>
  </section>
</template>
