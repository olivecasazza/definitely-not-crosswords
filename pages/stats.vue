<script setup lang="ts">
import { ref, computed } from 'vue'

const { data: user } = useAuth()
const { $client } = useNuxtApp()

// Selected Tab: 'leaderboard' | 'career' | 'h2h'
const activeTab = ref<'leaderboard' | 'career' | 'h2h'>('leaderboard')

// Queries
const { data: leaderboard, pending: pendingLeaderboard } = await $client.stats.getGlobalLeaderboard.useQuery()
const { data: careerStats, pending: pendingCareer } = await $client.stats.getUserStats.useQuery(
  computed(() => ({ email: user.value?.user?.email || '' })),
  { enabled: computed(() => !!user.value?.user?.email) }
)
const { data: players, pending: pendingPlayers } = await $client.stats.getAllPlayers.useQuery(
  computed(() => ({ excludeEmail: user.value?.user?.email || undefined }))
)

// H2H Selector & State
const selectedOpponentId = ref<string>('')
const { data: h2hData, pending: pendingH2H, refresh: refreshH2H } = await $client.stats.getHeadToHead.useQuery(
  computed(() => ({
    userEmail: user.value?.user?.email || '',
    opponentId: selectedOpponentId.value
  })),
  { enabled: computed(() => !!user.value?.user?.email && !!selectedOpponentId.value) }
)

// Watch opponent change to refresh H2H query
watch(selectedOpponentId, (val) => {
  if (val) {
    refreshH2H()
  }
})

// Format date helper
function formatDate(dateStr: string | Date) {
  if (!dateStr) return ''
  const d = new Date(dateStr)
  return d.toLocaleDateString(undefined, { 
    year: 'numeric', 
    month: 'short', 
    day: 'numeric' 
  })
}

// Initials helper
function getInitials(name?: string | null, email?: string | null) {
  const base = name || email || '?'
  return base.split(/[@\s]/)[0].substring(0, 2).toUpperCase()
}

// UI Rank Badge Class Helper
function getRankBadgeClass(index: number) {
  if (index === 0) return 'bg-[var(--pastel-yellow)] text-slate-900 border-[var(--pastel-yellow)] shadow-[0_0_12px_rgba(254,234,153,0.35)]'
  if (index === 1) return 'bg-slate-300 text-slate-900 border-slate-300 shadow-[0_0_12px_rgba(203,213,225,0.25)]'
  if (index === 2) return 'bg-amber-600 text-slate-100 border-amber-600'
  return 'bg-[var(--bg-cell-empty)] text-[var(--text-secondary)] border-[var(--border-app)]'
}
</script>

<template>
  <div class="flex-grow w-full max-w-5xl mx-auto px-4 sm:px-6 py-8 overflow-y-auto">
    <!-- Header -->
    <div class="flex flex-col md:flex-row md:items-center justify-between gap-4 border-b border-[var(--border-app)] pb-6 mb-8">
      <div class="font-mono">
        <h1 class="text-xl sm:text-2xl font-black uppercase tracking-widest text-[var(--color-primary)]">📊 observ & stats</h1>
        <p class="text-xs text-[var(--text-secondary)] mt-1 uppercase tracking-wider">Multiplayer Performance Metrics and Global Leaderboard</p>
      </div>

      <!-- Tab Buttons -->
      <div class="flex bg-[var(--bg-cell-empty)] border border-[var(--border-app)] rounded-lg p-1 font-mono text-xs max-w-max self-start md:self-auto">
        <button 
          @click="activeTab = 'leaderboard'"
          :class="['px-3 py-1.5 rounded uppercase tracking-wider font-bold transition-all duration-150', activeTab === 'leaderboard' ? 'bg-[var(--bg-card)] border border-[var(--border-app)] text-[var(--text-primary)]' : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]']"
        >
          🏆 Global
        </button>
        <button 
          @click="activeTab = 'career'"
          :class="['px-3 py-1.5 rounded uppercase tracking-wider font-bold transition-all duration-150', activeTab === 'career' ? 'bg-[var(--bg-card)] border border-[var(--border-app)] text-[var(--text-primary)]' : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]']"
        >
          👤 Career
        </button>
        <button 
          @click="activeTab = 'h2h'"
          :class="['px-3 py-1.5 rounded uppercase tracking-wider font-bold transition-all duration-150', activeTab === 'h2h' ? 'bg-[var(--bg-card)] border border-[var(--border-app)] text-[var(--text-primary)]' : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)]']"
        >
          ⚔️ Compare
        </button>
      </div>
    </div>

    <!-- TAB 1: Global Leaderboard -->
    <div v-if="activeTab === 'leaderboard'" class="flex flex-col gap-6 font-mono">
      <div v-if="pendingLeaderboard" class="flex flex-col items-center justify-center py-20">
        <LoadingBar />
        <span class="mt-4 text-xs uppercase tracking-widest text-[var(--text-secondary)]">Fetching rankings...</span>
      </div>

      <div v-else-if="!leaderboard?.length" class="app-card p-12 text-center text-xs text-[var(--text-secondary)]">
        No completed games or player statistics available yet. Play a game to record stats!
      </div>

      <div v-else class="flex flex-col gap-4">
        <!-- Top 3 Podium Cards -->
        <div class="grid grid-cols-1 sm:grid-cols-3 gap-4 mb-2">
          <!-- 2nd Place -->
          <div v-if="leaderboard[1]" class="app-card p-5 flex flex-col items-center justify-center text-center gap-2 border-slate-700/50 order-2 sm:order-1 bg-gradient-to-b from-[rgba(203,213,225,0.01)] to-transparent">
            <div class="w-10 h-10 rounded-lg bg-slate-300 text-slate-900 font-bold border border-slate-300 flex items-center justify-center text-sm shadow-[0_0_12px_rgba(203,213,225,0.15)]">2</div>
            <span class="text-sm font-bold truncate max-w-full text-[var(--text-primary)] mt-1">
              {{ leaderboard[1].name }}
            </span>
            <span class="text-[10px] text-[var(--pastel-yellow)] font-black uppercase">{{ leaderboard[1].totalScore }} pts</span>
            <span class="text-[9px] text-[var(--text-secondary)] uppercase">{{ leaderboard[1].gamesPlayed }} games · {{ leaderboard[1].accuracy }}% Acc</span>
          </div>

          <!-- 1st Place -->
          <div v-if="leaderboard[0]" class="app-card p-6 flex flex-col items-center justify-center text-center gap-2 border-[var(--pastel-yellow)]/30 order-1 sm:order-2 bg-gradient-to-b from-[rgba(254,234,153,0.03)] to-transparent scale-105 shadow-[0_0_15px_rgba(254,234,153,0.05)]">
            <div class="w-12 h-12 rounded-lg bg-[var(--pastel-yellow)] text-slate-900 font-bold border border-[var(--pastel-yellow)] flex items-center justify-center text-base shadow-[0_0_15px_rgba(254,234,153,0.3)]">👑</div>
            <span class="text-base font-black truncate max-w-full text-[var(--text-primary)] mt-1">
              {{ leaderboard[0].name }}
            </span>
            <span class="text-sm text-[var(--pastel-yellow)] font-black uppercase">{{ leaderboard[0].totalScore }} pts</span>
            <span class="text-[9px] text-[var(--text-secondary)] uppercase">{{ leaderboard[0].gamesPlayed }} games · {{ leaderboard[0].accuracy }}% Acc</span>
          </div>

          <!-- 3rd Place -->
          <div v-if="leaderboard[2]" class="app-card p-5 flex flex-col items-center justify-center text-center gap-2 border-amber-800/30 order-3 sm:order-3 bg-gradient-to-b from-[rgba(217,119,6,0.01)] to-transparent">
            <div class="w-10 h-10 rounded-lg bg-amber-600 text-slate-100 font-bold border border-amber-600 flex items-center justify-center text-sm">3</div>
            <span class="text-sm font-bold truncate max-w-full text-[var(--text-primary)] mt-1">
              {{ leaderboard[2].name }}
            </span>
            <span class="text-[10px] text-[var(--pastel-yellow)] font-black uppercase">{{ leaderboard[2].totalScore }} pts</span>
            <span class="text-[9px] text-[var(--text-secondary)] uppercase">{{ leaderboard[2].gamesPlayed }} games · {{ leaderboard[2].accuracy }}% Acc</span>
          </div>
        </div>

        <!-- Remaining Rankings Table -->
        <div class="app-card overflow-hidden">
          <div class="overflow-x-auto">
            <table class="w-full text-left border-collapse">
              <thead>
                <tr class="border-b border-[var(--border-app)] text-[var(--text-secondary)] text-[10px] uppercase tracking-wider bg-[var(--bg-cell-empty)]">
                  <th class="py-3.5 px-4 font-bold">Rank</th>
                  <th class="py-3.5 px-4 font-bold">Player</th>
                  <th class="py-3.5 px-4 font-bold text-center">Games</th>
                  <th class="py-3.5 px-4 font-bold text-center">Accuracy</th>
                  <th class="py-3.5 px-4 font-bold text-right">Career Score</th>
                </tr>
              </thead>
              <tbody>
                <tr 
                  v-for="(player, index) in leaderboard" 
                  :key="player.id"
                  :class="[
                    'border-b border-[var(--border-app)]/50 hover:bg-[var(--bg-cell-empty)]/30 text-xs font-mono transition-all duration-150',
                    user?.user?.email === player.email ? 'bg-[rgba(254,234,153,0.02)] font-bold' : ''
                  ]"
                >
                  <!-- Rank -->
                  <td class="py-3.5 px-4">
                    <span :class="['inline-flex w-6 h-6 rounded font-bold items-center justify-center text-[10px] border', getRankBadgeClass(index)]">
                      {{ index + 1 }}
                    </span>
                  </td>

                  <!-- Player Name -->
                  <td class="py-3.5 px-4">
                    <span class="truncate flex items-center gap-1.5 max-w-[200px] sm:max-w-xs">
                      {{ player.name }}
                      <span v-if="user?.user?.email === player.email" class="text-[8px] font-black border border-[var(--pastel-yellow)]/30 text-[var(--pastel-yellow)] px-1 rounded uppercase tracking-wider">YOU</span>
                    </span>
                  </td>

                  <!-- Games -->
                  <td class="py-3.5 px-4 text-center text-[var(--text-secondary)]">
                    {{ player.gamesPlayed }}
                  </td>

                  <!-- Accuracy -->
                  <td class="py-3.5 px-4 text-center">
                    <span :class="player.accuracy >= 75 ? 'text-[var(--pastel-green)]' : player.accuracy >= 45 ? 'text-[var(--pastel-yellow)]' : 'text-[var(--pastel-red)]'">
                      {{ player.accuracy }}%
                    </span>
                  </td>

                  <!-- Career Score -->
                  <td class="py-3.5 px-4 text-right font-black text-[var(--pastel-yellow)] text-sm">
                    {{ player.totalScore }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>

    <!-- TAB 2: Career Stats -->
    <div v-else-if="activeTab === 'career'" class="flex flex-col gap-6 font-mono">
      <div v-if="pendingCareer" class="flex flex-col items-center justify-center py-20">
        <LoadingBar />
        <span class="mt-4 text-xs uppercase tracking-widest text-[var(--text-secondary)]">Compiling career file...</span>
      </div>

      <div v-else-if="!careerStats || careerStats.gamesPlayed === 0" class="app-card p-12 text-center text-xs flex flex-col items-center gap-4">
        <span>👤 No games played yet on this profile.</span>
        <NuxtLink to="/games" class="app-btn font-bold uppercase tracking-wider border-[var(--pastel-yellow)]/30 text-[var(--pastel-yellow)] hover:bg-[rgba(254,234,153,0.02)]">
          Launch a Game
        </NuxtLink>
      </div>

      <div v-else class="flex flex-col gap-8">
        <!-- Aggregated Cards Row -->
        <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
          <!-- Global Rank -->
          <div class="app-card p-4 flex flex-col justify-between h-24">
            <span class="text-[9px] uppercase text-[var(--text-secondary)] tracking-wider">Global Rank</span>
            <div class="flex items-baseline gap-1">
              <span class="text-2xl font-black text-[var(--pastel-yellow)]">#{{ careerStats.globalRank }}</span>
              <span class="text-[9px] text-[var(--text-secondary)] uppercase">of {{ careerStats.totalPlayers }}</span>
            </div>
          </div>
          
          <!-- Career Score -->
          <div class="app-card p-4 flex flex-col justify-between h-24">
            <span class="text-[9px] uppercase text-[var(--text-secondary)] tracking-wider">Career Score</span>
            <span class="text-2xl font-black text-[var(--pastel-yellow)]">{{ careerStats.totalScore }}</span>
          </div>

          <!-- Games Played -->
          <div class="app-card p-4 flex flex-col justify-between h-24">
            <span class="text-[9px] uppercase text-[var(--text-secondary)] tracking-wider">Games Played</span>
            <span class="text-2xl font-black text-[var(--text-primary)]">{{ careerStats.gamesPlayed }}</span>
          </div>

          <!-- Precision Accuracy -->
          <div class="app-card p-4 flex flex-col justify-between h-24">
            <span class="text-[9px] uppercase text-[var(--text-secondary)] tracking-wider">Solve Accuracy</span>
            <span class="text-2xl font-black text-[var(--pastel-green)]">{{ careerStats.accuracy }}%</span>
          </div>
        </div>

        <!-- Career Guess Breakdown -->
        <div class="app-card p-5 flex flex-col gap-4">
          <span class="text-xs uppercase text-[var(--text-secondary)] font-bold">Accuracy Breakdown</span>
          
          <div class="flex flex-col gap-2">
            <!-- Labels -->
            <div class="flex justify-between items-center text-[10px] text-[var(--text-secondary)] uppercase">
              <span>Correct: {{ careerStats.totalCorrect }}</span>
              <span>Incorrect: {{ careerStats.totalIncorrect }}</span>
            </div>

            <!-- Horizontal Precision Bar -->
            <div class="h-3 w-full bg-[var(--bg-cell-empty)] border border-[var(--border-app)] rounded-full overflow-hidden flex">
              <div 
                :style="{ width: `${careerStats.accuracy}%` }" 
                class="h-full bg-[var(--pastel-green)] transition-all duration-300"
              ></div>
              <div 
                :style="{ width: `${100 - careerStats.accuracy}%` }" 
                class="h-full bg-[var(--pastel-red)] transition-all duration-300"
              ></div>
            </div>

            <!-- Percentage info -->
            <span class="text-[9px] text-[var(--text-secondary)] mt-1 leading-normal uppercase">
              Your overall ratio is <span class="text-[var(--pastel-green)] font-bold">{{ careerStats.totalCorrect }} correct guesses</span> out of <span class="text-[var(--text-primary)] font-bold">{{ careerStats.totalCorrect + careerStats.totalIncorrect }} total guesses</span>.
            </span>
          </div>
        </div>

        <!-- Recent Games Played List -->
        <div class="flex flex-col gap-4">
          <span class="text-xs font-bold uppercase tracking-wider text-[var(--text-secondary)] px-1">🕒 Match Log History</span>
          
          <div class="flex flex-col gap-3">
            <div 
              v-for="game in careerStats.recentGames" 
              :key="game!.id"
              class="app-card p-4 sm:p-5 flex flex-col sm:flex-row sm:items-center justify-between gap-4 transition-all hover:border-[var(--border-hover)]"
            >
              <!-- Info -->
              <div class="flex flex-col min-w-0">
                <span class="text-sm font-bold text-[var(--text-primary)] truncate">{{ game!.title }}</span>
                <span class="text-[9px] uppercase tracking-wider text-[var(--text-secondary)] mt-1">
                  Played on {{ formatDate(game!.createdAt) }}
                </span>
              </div>

              <!-- Scoring details -->
              <div class="flex items-center justify-between sm:justify-end gap-6 sm:gap-10 text-right">
                <!-- Mini Stats -->
                <div class="flex flex-col text-left sm:text-right">
                  <span class="text-[9px] uppercase text-[var(--text-secondary)]">Guesses</span>
                  <span class="text-xs font-semibold text-[var(--text-primary)]">
                    <span class="text-[var(--pastel-green)]">{{ game!.correctGuesses }}</span> / <span class="text-[var(--pastel-red)]">{{ game!.incorrectGuesses }}</span>
                  </span>
                </div>

                <!-- Match Rank -->
                <div class="flex flex-col">
                  <span class="text-[9px] uppercase text-[var(--text-secondary)]">Place</span>
                  <span class="text-xs font-bold text-[var(--text-primary)]">
                    #{{ game!.rank }} <span class="text-[9px] text-[var(--text-secondary)] font-normal uppercase">of {{ game!.totalParticipants }}</span>
                  </span>
                </div>

                <!-- Score -->
                <div class="flex flex-col min-w-[50px] pl-4 sm:pl-6 border-l border-[var(--border-app)]">
                  <span class="text-[9px] uppercase text-[var(--text-secondary)]">Score</span>
                  <span class="text-sm font-black text-[var(--pastel-yellow)]">{{ game!.score }}</span>
                </div>

                <!-- Action Button -->
                <NuxtLink :to="`/game/${game!.id}/completed`" class="app-btn text-[10px] py-1 px-2.5 uppercase font-bold text-center">
                  Review
                </NuxtLink>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- TAB 3: Head to Head Compare -->
    <div v-else-if="activeTab === 'h2h'" class="flex flex-col gap-6 font-mono">
      
      <!-- Selection Bar -->
      <div class="app-card p-5 flex flex-col sm:flex-row items-center justify-between gap-4">
        <div class="flex flex-col font-mono">
          <span class="text-xs font-bold uppercase tracking-wider">Select Opponent</span>
          <span class="text-[9px] text-[var(--text-secondary)] uppercase mt-0.5">Compare your career performance side-by-side</span>
        </div>
        
        <!-- Dropdown select -->
        <select 
          v-model="selectedOpponentId"
          class="app-input px-3 py-2 text-xs font-mono max-w-xs w-full bg-[var(--bg-card)] uppercase font-bold"
        >
          <option value="" disabled>-- CHOOSE PLAYER --</option>
          <option 
            v-for="p in players" 
            :key="p.id" 
            :value="p.id"
          >
            {{ p.name || p.email }}
          </option>
        </select>
      </div>

      <!-- No opponent selected state -->
      <div v-if="!selectedOpponentId" class="app-card p-12 text-center text-xs text-[var(--text-secondary)]">
        Select another player from the dropdown to unlock head-to-head comparison records.
      </div>

      <!-- Loading H2H stats -->
      <div v-else-if="pendingH2H" class="flex flex-col items-center justify-center py-20">
        <LoadingBar />
        <span class="mt-4 text-xs uppercase tracking-widest text-[var(--text-secondary)]">Computing combat records...</span>
      </div>

      <!-- Main Comparison Screen -->
      <div v-else-if="h2hData" class="flex flex-col gap-8">
        
        <!-- H2H Record Banner -->
        <div class="app-card p-6 border-[var(--pastel-yellow)]/20 text-center flex flex-col items-center justify-center gap-3 bg-[rgba(254,234,153,0.01)]">
          <h3 class="text-[10px] text-[var(--text-secondary)] uppercase font-bold tracking-widest">CO-OP MATCH RECORD</h3>
          <div class="flex items-center gap-4 text-xl sm:text-2xl font-black">
            <span class="text-[var(--pastel-green)]">{{ h2hData.record.wins }} W</span>
            <span class="text-[var(--text-secondary)] opacity-30 text-lg font-normal">—</span>
            <span class="text-[var(--pastel-red)]">{{ h2hData.record.losses }} L</span>
            <span class="text-[var(--text-secondary)] opacity-30 text-lg font-normal">—</span>
            <span class="text-[var(--text-secondary)]">{{ h2hData.record.ties }} T</span>
          </div>
          <span class="text-[9px] uppercase tracking-wider text-[var(--text-secondary)] border-t border-[var(--border-app)] pt-2.5 w-full max-w-sm">
            Total Shared Matches: <span class="text-[var(--text-primary)] font-bold">{{ h2hData.gamesPlayed }}</span>
          </span>
        </div>

        <!-- Comparative Side-by-Side Statistics Grid -->
        <div class="flex flex-col gap-4">
          <span class="text-xs font-bold uppercase tracking-wider text-[var(--text-secondary)] px-1">⚔️ Stat Comparison</span>
          
          <div class="app-card p-6 flex flex-col gap-6 font-mono text-xs">
            
            <!-- Games Played Row -->
            <div class="flex flex-col gap-2">
              <div class="flex justify-between font-bold">
                <span class="uppercase">Games Played Together</span>
                <span class="text-[var(--pastel-yellow)]">{{ h2hData.gamesPlayed }}</span>
              </div>
              <div class="h-2 w-full bg-[var(--bg-cell-empty)] border border-[var(--border-app)] rounded-full overflow-hidden flex">
                <div class="h-full bg-[var(--pastel-yellow)] w-full"></div>
              </div>
            </div>

            <!-- Shared Score -->
            <div class="flex flex-col gap-2">
              <div class="flex justify-between font-bold">
                <span class="uppercase">Total Shared Score</span>
                <span class="flex gap-4">
                  <span class="text-[var(--text-primary)]">You: {{ h2hData.scores.userTotal }}</span>
                  <span class="text-[var(--text-secondary)] opacity-40">|</span>
                  <span class="text-[var(--text-secondary)]">Them: {{ h2hData.scores.opponentTotal }}</span>
                </span>
              </div>
              <!-- Multi-color comparison bar -->
              <div class="h-2 w-full bg-[var(--bg-cell-empty)] border border-[var(--border-app)] rounded-full overflow-hidden flex">
                <div 
                  :style="{ 
                    width: `${
                      h2hData.scores.userTotal + h2hData.scores.opponentTotal > 0 
                        ? (h2hData.scores.userTotal / (h2hData.scores.userTotal + h2hData.scores.opponentTotal)) * 100 
                        : 50
                    }%` 
                  }"
                  class="h-full bg-[var(--pastel-yellow)]"
                ></div>
                <div 
                  :style="{ 
                    width: `${
                      h2hData.scores.userTotal + h2hData.scores.opponentTotal > 0 
                        ? (h2hData.scores.opponentTotal / (h2hData.scores.userTotal + h2hData.scores.opponentTotal)) * 100 
                        : 50
                    }%` 
                  }"
                  class="h-full bg-slate-500"
                ></div>
              </div>
              <span class="text-[9px] text-[var(--text-secondary)] uppercase mt-0.5">
                Yellow: You ({{ h2hData.scores.userTotal }}) · Grey: Opponent ({{ h2hData.scores.opponentTotal }})
              </span>
            </div>

            <!-- Average Match Score -->
            <div class="flex flex-col gap-2">
              <div class="flex justify-between font-bold">
                <span class="uppercase">Average Match Score</span>
                <span class="flex gap-4">
                  <span class="text-[var(--text-primary)]">You: {{ h2hData.scores.userAvg }}</span>
                  <span class="text-[var(--text-secondary)] opacity-40">|</span>
                  <span class="text-[var(--text-secondary)]">Them: {{ h2hData.scores.opponentAvg }}</span>
                </span>
              </div>
              <div class="h-2 w-full bg-[var(--bg-cell-empty)] border border-[var(--border-app)] rounded-full overflow-hidden flex">
                <div 
                  :style="{ 
                    width: `${
                      h2hData.scores.userAvg + h2hData.scores.opponentAvg > 0 
                        ? (h2hData.scores.userAvg / (h2hData.scores.userAvg + h2hData.scores.opponentAvg)) * 100 
                        : 50
                    }%` 
                  }"
                  class="h-full bg-[var(--pastel-yellow)]"
                ></div>
                <div 
                  :style="{ 
                    width: `${
                      h2hData.scores.userAvg + h2hData.scores.opponentAvg > 0 
                        ? (h2hData.scores.opponentAvg / (h2hData.scores.userAvg + h2hData.scores.opponentAvg)) * 100 
                        : 50
                    }%` 
                  }"
                  class="h-full bg-slate-500"
                ></div>
              </div>
            </div>

            <!-- Accuracy -->
            <div class="flex flex-col gap-2">
              <div class="flex justify-between font-bold">
                <span class="uppercase">Shared Guess Accuracy</span>
                <span class="flex gap-4">
                  <span class="text-[var(--pastel-green)]">You: {{ h2hData.accuracy.user }}%</span>
                  <span class="text-[var(--text-secondary)] opacity-40">|</span>
                  <span class="text-slate-400">Them: {{ h2hData.accuracy.opponent }}%</span>
                </span>
              </div>
              <div class="h-2 w-full bg-[var(--bg-cell-empty)] border border-[var(--border-app)] rounded-full overflow-hidden flex">
                <div 
                  :style="{ 
                    width: `${
                      h2hData.accuracy.user + h2hData.accuracy.opponent > 0 
                        ? (h2hData.accuracy.user / (h2hData.accuracy.user + h2hData.accuracy.opponent)) * 100 
                        : 50
                    }%` 
                  }"
                  class="h-full bg-[var(--pastel-green)]"
                ></div>
                <div 
                  :style="{ 
                    width: `${
                      h2hData.accuracy.user + h2hData.accuracy.opponent > 0 
                        ? (h2hData.accuracy.opponent / (h2hData.accuracy.user + h2hData.accuracy.opponent)) * 100 
                        : 50
                    }%` 
                  }"
                  class="h-full bg-slate-500"
                ></div>
              </div>
            </div>

          </div>
        </div>

        <!-- Shared Matches History Log -->
        <div class="flex flex-col gap-4">
          <span class="text-xs font-bold uppercase tracking-wider text-[var(--text-secondary)] px-1">📖 Combat Match Log</span>
          
          <div v-if="!h2hData.matches?.length" class="app-card p-8 text-center text-xs text-[var(--text-secondary)]">
            You haven't played any co-op crossword games with this player yet.
          </div>

          <div v-else class="flex flex-col gap-3">
            <div 
              v-for="match in h2hData.matches" 
              :key="match.gameId"
              class="app-card p-4 sm:p-5 flex flex-col sm:flex-row sm:items-center justify-between gap-4 transition-all font-mono text-xs"
            >
              <!-- Info -->
              <div class="flex flex-col min-w-0">
                <span class="text-sm font-bold text-[var(--text-primary)] truncate">{{ match.title }}</span>
                <span class="text-[9px] uppercase tracking-wider text-[var(--text-secondary)] mt-1">
                  Played on {{ formatDate(match.createdAt) }}
                </span>
              </div>

              <!-- Shared details -->
              <div class="flex items-center justify-between sm:justify-end gap-6 sm:gap-10 text-right">
                <!-- Scores compare -->
                <div class="flex flex-col">
                  <span class="text-[9px] uppercase text-[var(--text-secondary)]">Match Scores</span>
                  <span class="text-xs font-bold text-[var(--text-primary)]">
                    You <span class="text-[var(--pastel-yellow)]">{{ match.userScore }}</span> — <span class="text-[var(--text-secondary)]">{{ match.opponentScore }}</span> Them
                  </span>
                </div>

                <!-- Match Outcome -->
                <div class="flex flex-col min-w-[70px]">
                  <span class="text-[9px] uppercase text-[var(--text-secondary)]">Outcome</span>
                  <span 
                    :class="[
                      'text-xs font-bold uppercase tracking-wider',
                      match.result === 'WIN' ? 'text-[var(--pastel-green)]' : match.result === 'LOSS' ? 'text-[var(--pastel-red)]' : 'text-[var(--text-secondary)]'
                    ]"
                  >
                    {{ match.result }}
                  </span>
                </div>

                <!-- Action Button -->
                <NuxtLink :to="`/game/${match.gameId}/completed`" class="app-btn text-[10px] py-1 px-2.5 uppercase font-bold text-center">
                  Stats
                </NuxtLink>
              </div>
            </div>
          </div>
        </div>

      </div>
    </div>
  </div>
</template>
