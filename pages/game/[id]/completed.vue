<script setup lang="ts">
import { computed } from 'vue'

const { data: user } = useAuth()
const { $client } = useNuxtApp()
const route = useRoute()

// Fetch completed game details by ID
const { data: gameData, pending, error } = await $client.stats.getCompletedGame.useQuery({
  id: route.params.id as string
})

// Compute sorted rankings from the member scores
const rankings = computed(() => {
  if (!gameData.value?.gameStats?.memberScores) return []
  return [...gameData.value.gameStats.memberScores].sort((a, b) => b.score - a.score)
})

// Format date helper
function formatDate(dateStr: string | Date) {
  if (!dateStr) return ''
  const d = new Date(dateStr)
  return d.toLocaleDateString(undefined, { 
    year: 'numeric', 
    month: 'long', 
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit'
  })
}

// Get initials for profile placeholder
function getInitials(name?: string | null, email?: string | null) {
  const base = name || email || '?'
  return base.split(/[@\s]/)[0].substring(0, 2).toUpperCase()
}

// Check if this member is the current user
function isCurrentUser(memberEmail?: string | null) {
  return user.value?.user?.email && memberEmail && user.value.user.email === memberEmail
}

// UI Rank Badge Class Helper
function getRankBadgeClass(index: number) {
  if (index === 0) return 'bg-[var(--pastel-yellow)] text-slate-900 border-[var(--pastel-yellow)] shadow-[0_0_12px_rgba(254,234,153,0.35)]'
  if (index === 1) return 'bg-slate-300 text-slate-900 border-slate-300 shadow-[0_0_12px_rgba(203,213,225,0.25)]'
  if (index === 2) return 'bg-amber-600 text-slate-100 border-amber-600'
  return 'bg-[var(--bg-cell-empty)] text-[var(--text-secondary)] border-[var(--border-app)]'
}

function getRankName(index: number) {
  if (index === 0) return '🏆 1ST PLACE'
  if (index === 1) return '🥈 2ND PLACE'
  if (index === 2) return '🥉 3RD PLACE'
  return `${index + 1}TH PLACE`
}
</script>

<template>
  <div class="flex-grow w-full max-w-4xl mx-auto px-4 sm:px-6 py-8 overflow-y-auto">
    <!-- Loading State -->
    <div v-if="pending" class="flex flex-col items-center justify-center py-20">
      <LoadingBar />
      <span class="mt-4 font-mono text-xs uppercase tracking-widest text-[var(--text-secondary)]">Analyzing results...</span>
    </div>

    <!-- Error State -->
    <div v-else-if="error || !gameData" class="flex flex-col items-center justify-center py-12 text-center">
      <div class="p-6 app-card max-w-md w-full flex flex-col gap-4 font-mono">
        <span class="text-sm font-bold text-[var(--pastel-red)] uppercase tracking-wider">⚠️ Error Loading Match Details</span>
        <p class="text-xs text-[var(--text-secondary)]">The requested completed game could not be found or you do not have permission to view it.</p>
        <NuxtLink to="/games" class="app-btn text-center mt-2">Back to Lobby</NuxtLink>
      </div>
    </div>

    <!-- Main Content -->
    <div v-else class="flex flex-col gap-8">
      
      <!-- Victory Banner -->
      <div class="app-card relative p-8 overflow-hidden text-center flex flex-col items-center justify-center gap-3 border-[var(--pastel-green)]/30 shadow-[0_0_20px_rgba(168,230,207,0.08)] bg-gradient-to-b from-[rgba(168,230,207,0.03)] to-transparent">
        <div class="absolute inset-0 bg-[radial-gradient(ellipse_at_center,rgba(168,230,207,0.05),transparent)] pointer-events-none"></div>
        <div class="w-16 h-16 rounded-full bg-[rgba(168,230,207,0.1)] border border-[var(--pastel-green)]/30 flex items-center justify-center text-3xl animate-bounce">
          🎉
        </div>
        <h1 class="text-xl sm:text-2xl font-bold uppercase tracking-widest text-[var(--pastel-green)]">Crossword Solved!</h1>
        <p class="text-xs font-mono text-[var(--text-secondary)] uppercase tracking-wider">
          Game Room: <span class="text-[var(--text-primary)] font-bold">{{ gameData.game.title }}</span>
        </p>
        <div class="flex items-center gap-2 mt-2 font-mono text-[10px] text-[var(--text-secondary)] border-t border-[var(--border-app)] pt-3 w-full max-w-sm justify-center">
          <span>COMPLETED:</span>
          <span class="text-[var(--text-primary)] font-bold">{{ formatDate(gameData.createdAt) }}</span>
        </div>
      </div>

      <!-- Scoreboard Grid -->
      <div class="grid grid-cols-1 md:grid-cols-12 gap-8 items-start">
        
        <!-- Left: Rankings List (Leaderboard) -->
        <div class="md:col-span-8 flex flex-col gap-4">
          <div class="flex items-center justify-between font-mono px-1">
            <h2 class="text-xs font-bold uppercase tracking-wider text-[var(--text-secondary)]">🏆 Match Standings</h2>
            <span class="text-[10px] text-[var(--text-secondary)] uppercase">{{ rankings.length }} Players</span>
          </div>

          <div class="flex flex-col gap-3">
            <div 
              v-for="(scoreRecord, index) in rankings" 
              :key="scoreRecord.id"
              :class="[
                'app-card p-4 sm:p-5 flex items-center justify-between gap-4 transition-all duration-300 font-mono',
                isCurrentUser(scoreRecord.member.user.email)
                  ? 'border-[var(--pastel-yellow)]/40 shadow-[0_0_15px_rgba(254,234,153,0.05)] bg-[rgba(254,234,153,0.02)]'
                  : ''
              ]"
            >
              <!-- Left side: Rank + Avatar + Name -->
              <div class="flex items-center gap-4 min-w-0">
                <!-- Rank Badge -->
                <div :class="['w-8 h-8 rounded-lg border font-bold flex items-center justify-center text-xs flex-shrink-0', getRankBadgeClass(index)]">
                  {{ index + 1 }}
                </div>

                <!-- Avatar Circle -->
                <div class="w-9 h-9 rounded-full bg-[var(--bg-cell-empty)] border border-[var(--border-app)] flex items-center justify-center text-xs font-bold text-[var(--text-secondary)] flex-shrink-0">
                  {{ getInitials(scoreRecord.member.user.name, scoreRecord.member.user.email) }}
                </div>

                <!-- Player Details -->
                <div class="flex flex-col min-w-0">
                  <span class="text-sm font-bold truncate flex items-center gap-1.5">
                    {{ scoreRecord.member.user.name || scoreRecord.member.user.email || "Anonymous" }}
                    <span v-if="isCurrentUser(scoreRecord.member.user.email)" class="text-[9px] font-bold tracking-widest text-[var(--pastel-yellow)] border border-[var(--pastel-yellow)]/40 px-1 py-[1px] rounded uppercase">YOU</span>
                    <span v-if="scoreRecord.member.isOwner" class="text-[9px] font-bold text-[var(--text-secondary)] opacity-60">👑</span>
                  </span>
                  <span class="text-[9px] uppercase tracking-wider text-[var(--text-secondary)] mt-0.5">
                    {{ getRankName(index) }}
                  </span>
                </div>
              </div>

              <!-- Right side: Score & Accuracy -->
              <div class="flex items-center gap-6 sm:gap-10 flex-shrink-0 text-right">
                <!-- Accuracy Gauge -->
                <div class="hidden sm:flex flex-col">
                  <span class="text-[10px] uppercase text-[var(--text-secondary)]">Accuracy</span>
                  <span class="text-xs font-bold text-[var(--text-primary)]">
                    {{ 
                      (scoreRecord.correctGuesses + scoreRecord.incorrectGuesses) > 0 
                        ? Math.round((scoreRecord.correctGuesses / (scoreRecord.correctGuesses + scoreRecord.incorrectGuesses)) * 100) 
                        : 0 
                    }}%
                  </span>
                </div>

                <!-- Guesses Breakdowns -->
                <div class="flex flex-col">
                  <span class="text-[10px] uppercase text-[var(--text-secondary)]">Guesses</span>
                  <span class="text-xs font-bold flex items-center gap-1.5 justify-end">
                    <span class="text-[var(--pastel-green)]">{{ scoreRecord.correctGuesses }}</span>
                    <span class="text-[var(--text-secondary)] opacity-50">/</span>
                    <span class="text-[var(--pastel-red)]">{{ scoreRecord.incorrectGuesses }}</span>
                  </span>
                </div>

                <!-- Score Big -->
                <div class="flex flex-col pl-4 sm:pl-6 border-l border-[var(--border-app)] min-w-[70px]">
                  <span class="text-[10px] uppercase text-[var(--text-secondary)]">Score</span>
                  <span class="text-base font-black text-[var(--pastel-yellow)]">{{ scoreRecord.score }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- Right: Player Stats Card & Navigation -->
        <div class="md:col-span-4 flex flex-col gap-6">
          <div class="font-mono text-xs font-bold uppercase tracking-wider text-[var(--text-secondary)] px-1">
            ⚡ Performance Insights
          </div>

          <!-- Stats Overview Card -->
          <div class="app-card p-6 flex flex-col gap-5 font-mono">
            <h3 class="text-xs font-bold uppercase tracking-wider border-b border-[var(--border-app)] pb-3">Crossword Metrics</h3>
            
            <div class="flex flex-col gap-4">
              <!-- Game Mode -->
              <div class="flex justify-between items-center text-xs">
                <span class="text-[var(--text-secondary)] uppercase">Source Mode</span>
                <span class="font-bold uppercase bg-[var(--bg-cell-empty)] border border-[var(--border-app)] px-2 py-0.5 rounded text-[10px]">
                  {{ gameData.game.source }}
                </span>
              </div>
              
              <!-- Total Questions -->
              <div class="flex justify-between items-center text-xs">
                <span class="text-[var(--text-secondary)] uppercase">Total Clues</span>
                <span class="font-bold text-[var(--text-primary)]">
                  {{ gameData.game.questions?.length || 0 }} Clues
                </span>
              </div>

              <!-- Total Game Actions -->
              <div class="flex justify-between items-center text-xs">
                <span class="text-[var(--text-secondary)] uppercase">Total Guesses</span>
                <span class="font-bold text-[var(--text-primary)]">
                  {{ 
                    rankings.reduce((sum, r) => sum + r.correctGuesses + r.incorrectGuesses, 0)
                  }}
                </span>
              </div>

              <!-- Correct Rate (Global) -->
              <div class="flex justify-between items-center text-xs">
                <span class="text-[var(--text-secondary)] uppercase">Solve Precision</span>
                <span class="font-bold text-[var(--pastel-green)]">
                  {{
                    (() => {
                      const correct = rankings.reduce((sum, r) => sum + r.correctGuesses, 0);
                      const total = rankings.reduce((sum, r) => sum + r.correctGuesses + r.incorrectGuesses, 0);
                      return total > 0 ? Math.round((correct / total) * 100) : 0;
                    })()
                  }}%
                </span>
              </div>
            </div>

            <!-- Nice visual divider -->
            <div class="h-[1px] bg-[var(--border-app)] w-full"></div>

            <div class="flex flex-col gap-2.5">
              <NuxtLink to="/stats" class="app-btn flex items-center justify-center gap-2 border-[var(--pastel-yellow)]/30 text-[var(--pastel-yellow)] hover:bg-[rgba(254,234,153,0.02)] py-2.5 font-bold uppercase tracking-wider text-center text-xs">
                📊 Career Stats Dashboard
              </NuxtLink>
              
              <NuxtLink to="/games" class="app-btn flex items-center justify-center gap-2 py-2.5 text-center text-xs font-bold uppercase tracking-wider">
                🎮 Back to Lobby
              </NuxtLink>
            </div>
          </div>

          <!-- Pro Tip box -->
          <div class="app-card p-5 bg-[rgba(254,234,153,0.02)] border-[var(--pastel-yellow)]/10 font-mono text-[10px] leading-relaxed text-[var(--text-secondary)] flex gap-2">
            <span class="text-sm">💡</span>
            <div>
              <span class="text-[var(--text-primary)] font-bold uppercase block mb-1">Scoring Mechanics</span>
              Each correct guess gives <span class="text-[var(--pastel-green)] font-bold">+10 pts</span>. However, every incorrect guess subtracts <span class="text-[var(--pastel-red)] font-bold">-2 pts</span>. Aim for perfect precision!
            </div>
          </div>

        </div>

      </div>

    </div>
  </div>
</template>
