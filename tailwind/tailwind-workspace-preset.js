const defaultTheme = require('tailwindcss/defaultTheme');
const themeColors = require('./colors.cjs');

const fontFamily = defaultTheme.fontFamily;
fontFamily['sans'] = [
  'Inconsolata',
  'monospace',
  'Courier New',
  'Roboto',
  'system-ui',
];

module.exports = {
  darkMode: 'class', // or 'media' or 'class'
  theme: {
    fontFamily,
    extend: {
      colors: themeColors,
    },
  },
};
