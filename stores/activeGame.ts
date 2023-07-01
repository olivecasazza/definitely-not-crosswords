// Pinia Store
import {
  ActiveGame,
  GameAction,
  GameMember,
  Question,
} from "@prisma/client";
import { Ref } from "nuxt/dist/app/compat/capi";
import { defineStore, storeToRefs } from "pinia";
import { BoardState, Cell } from "~/lib/game";
import { GetBoardSize } from "~/lib/game/boardSizeFromQuestions";
import {
  computeQuestionAnswerMap,
  WithComputedProperties,
} from "~/lib/game/question";
import { useUserStore } from "./user";
import { randomUUID } from "crypto";

export const useActiveGameStore = defineStore("activeGame", () => {
  // state
  const activeGame: Ref<ActiveGame> = ref({
    id: "",
    type: "",
    createdAt: new Date(),
    updatedAt: new Date(),
    gameId: "",
  } as ActiveGame);
  const questions = ref([] as WithComputedProperties<Question>[]);

  // reactive state
  const activeGameLoading = ref(true);
  const members = ref([] as GameMember[]);
  const actions = ref([] as GameAction[]);
  const selectedQuestion = ref({} as WithComputedProperties<Question> | null);
  const selectedDirection = ref("ACROSS" as ("ACROSS" | "DOWN") | null);

  // game action reactive state
  const gameActionData = ref([] as GameAction[]);

  // getters
  const acrossQuestions = computed(() =>
    questions.value.filter(
      (q: WithComputedProperties<Question>) =>
        q.direction === "ACROSS"
    )
  );
  const downQuestions = computed(() =>
    questions.value.filter(
      (q: WithComputedProperties<Question>) =>
        q.direction === "DOWN"
    )
  );
  const filteredQuestions = computed((): WithComputedProperties<Question>[] => {
    return selectedDirection.value == ("DOWN")
      ? downQuestions.value
      : selectedDirection.value == ("ACROSS")
      ? acrossQuestions.value
      : questions.value;
  });

  const boardSize = computed(() => GetBoardSize(questions.value));
  const boardState = computed(() =>
    BoardState.BoardStateFromActions(
      boardSize.value,
      actions.value,
      questions.value
    )
  );

  // actions
  async function load() {
    const route = useRoute();
    const activeGameId = route.params.id as string
    const { $client } = useNuxtApp();
    const data = await $client.activeGame.get.query({
      id: route.params.id as string,
    });
    if (!data) {
      return;
    }
    activeGame.value.id = data.id;
    activeGame.value.type = data.type;
    activeGame.value.createdAt = new Date(data.createdAt);
    activeGame.value.updatedAt = new Date(data.updatedAt);
    activeGame.value.gameId = data.gameId;
    actions.value = data.actions.map((a) => {
      return {
        ...a,
        activeGameId,
        submittedAt: new Date(a.submittedAt),
      };
    });
    questions.value = data.game.questions.map((q: Question) =>
      computeQuestionAnswerMap(q, actions.value)
    );
    members.value = data.gameMembers.map((gm) => {
      return {
        ...gm,
        activeGameId,
        completedGameId: gm.completedGameId,
        createdAt: new Date(gm.createdAt),
        updatedAt: new Date(gm.updatedAt),
      };
    });
    // init subscription to other user's actions
    $client.activeGame.onAddActions.subscribe({ activeGameId }, {
      onData(newActions: GameAction[]) {
        actions.value = [...actions.value, ...newActions];
        questions.value = data.game.questions.map((q: Question) =>
          computeQuestionAnswerMap(q, actions.value)
        );
      },
    });

    activeGameLoading.value = false;
  }

  function unSelect() {
    const isActionsModified = gameActionData.value.some((cell) => cell.state !== "");
    if(selectedQuestion.value && isActionsModified) submitActions("placeholder", selectedQuestion.value);
    selectedQuestion.value = null;
    gameActionData.value = [];
  }

  function filterDown() {
    if (selectedDirection.value === "DOWN") selectedDirection.value = null;
    else {
      unSelect();
      selectedDirection.value = "DOWN";
    }
  }

  function filterAcross() {
    if (selectedDirection.value === "ACROSS") selectedDirection.value = null;
    else {
      unSelect();
      selectedDirection.value = "ACROSS";
    }
  }

  function selectCoordinates(x: number, y: number) {
    const questionMatch = questions.value.find(
      (q: WithComputedProperties<Question>) =>
        q.direction === selectedDirection.value &&
        q.answerMap.some((cell: Cell) => x === cell.cordX && y === cell.cordY)
    );
    if (!questionMatch) {
      throw new Error("could not find matching questions");
    }
    selectQuestion(questionMatch);
  }

  function selectQuestion(question: WithComputedProperties<Question>) {
    gameActionData.value = question.answerMap.map((cell) => {
      return {
        id: "",
        type: "GameAction",
        submittedAt: new Date(),
        activeGameId: useRoute().params.id,
        actionType: "placeholder",
        cordX: cell.cordX,
        cordY: cell.cordY,
        previousState: cell?.modifications?.at(0)?.state || "",
        state: cell?.modifications?.at(0)?.state || "",
        userId: "",
      } as GameAction;
    });
    selectedQuestion.value = question;
  }

  async function submitActions(
    actionType: "placeholder" | "guess" | "cancel",
    question: WithComputedProperties<Question>
  ) {
    if (!gameActionData) {
      throw new Error("action was not defined during submit event.");
    }
    if (actionType === "guess")
      gameActionData.value = gameActionData.value.map((a) => {
        a.actionType = checkIfCorrect(question, gameActionData.value)
          ? ("correctGuess")
          : ("incorrectGuess");
        return a;
      });
    const { email } = storeToRefs(useUserStore());
    const route = useRoute();
    const { $client } = useNuxtApp();
    await $client.activeGame.addActions.mutate({
      activeGameId: route.params.id as string,
      userEmail: email.value as string,
      actions: gameActionData.value
    });
  }

  function checkIfCorrect(
    question: WithComputedProperties<Question>,
    actionData: GameAction[]
  ) {
    return question.answerMap.every(
      (q, index) => q.correctState === actionData[index].state
    );
  }

  return {
    activeGame,
    activeGameLoading,
    boardState,
    questions,
    selectedDirection,
    filteredQuestions,
    actions,
    gameActionData,
    selectedQuestion,
    load,
    selectQuestion,
    selectCoordinates,
    unSelect,
    filterDown,
    filterAcross,
    submitActions,
  };
});
