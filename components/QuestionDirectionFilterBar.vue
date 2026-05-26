<template>
    <div class="flex flex-row justify-center gap-4 py-4 w-full max-w-xl mx-auto">
        <button :class="['app-btn w-32 font-mono uppercase text-xs tracking-wider', direction === 'DOWN' ? 'app-btn-active font-bold' : '']" @click="filterDown">down</button>
        <button :class="['app-btn w-32 font-mono uppercase text-xs tracking-wider', direction === 'ACROSS' ? 'app-btn-active font-bold' : '']" @click="filterAcross">across</button>
    </div>
</template>

<script setup lang="ts">
import { storeToRefs } from 'pinia';
import { useActiveGameStore } from '~/stores/activeGame';

const activeGameStore = useActiveGameStore();
const { filterDown, filterAcross } = activeGameStore;
const { selectedDirection: direction } = storeToRefs(activeGameStore);
</script>