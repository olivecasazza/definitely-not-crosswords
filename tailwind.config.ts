export default {
  theme: {
    extend: {
      fontFamily: {
        sans: ['"Montserrat"', 'system-ui', 'sans-serif'],
        serif: ['"Montserrat"', 'system-ui', 'sans-serif'],
        mono: ['Inconsolata', 'monospace'],
      },
    },
  },
  variants: {},
  presets: [require('./tailwind/tailwind-workspace-preset')],
  content: [
    `assets/**/*.{vue,js,ts,css}`,
    `tailwind/**/*.{vue,js,ts,css}`,
    `components/**/*.{vue,js,ts}`,
    `layouts/**/*.vue`,
    `pages/**/*.vue`,
    `composables/**/*.{js,ts}`,
    `plugins/**/*.{js,ts}`,
    `utils/**/*.{js,ts}`,
    `App.{js,ts,vue}`,
    `app.{js,ts,vue}`,
    `Error.{js,ts,vue}`,
    `error.{js,ts,vue}`,
    `app.config.{js,ts}`
  ],
  plugins: [
    require('@tailwindcss/aspect-ratio'),
  ],
};
