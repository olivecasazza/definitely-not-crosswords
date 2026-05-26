<template>
  <div>
    <NuxtLayout>
      <NuxtPage />
    </NuxtLayout>
  </div>
</template>

<script setup lang="ts">
const isLight = useState('isLight', () => false);

onMounted(() => {
  const saved = localStorage.getItem('theme');
  const prefersLight = window.matchMedia('(prefers-color-scheme: light)').matches;
  if (saved === 'light' || (!saved && prefersLight)) {
    isLight.value = true;
    document.documentElement.classList.add('light');
  } else {
    isLight.value = false;
    document.documentElement.classList.remove('light');
  }
});

watch(isLight, (val) => {
  if (val) {
    document.documentElement.classList.add('light');
    localStorage.setItem('theme', 'light');
  } else {
    document.documentElement.classList.remove('light');
    localStorage.setItem('theme', 'dark');
  }
});
</script>