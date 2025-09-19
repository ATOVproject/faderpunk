import { heroui } from "@heroui/theme";

/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./node_modules/@heroui/theme/dist/components/button.js",
    "./node_modules/@heroui/theme/dist/components/input.js",
    "./node_modules/@heroui/theme/dist/components/modal.js",
    "./node_modules/@heroui/theme/dist/components/select.js",
    "./node_modules/@heroui/theme/dist/components/skeleton.js",
    "./node_modules/@heroui/theme/dist/components/switch.js",
  ],
  theme: {
    extend: {},
  },
  darkMode: "class",
  plugins: [
    heroui({
      layout: {
        radius: {
          small: "2px",
          medium: "4px",
          large: "8px",
        },
      },
      themes: {
        dark: {
          colors: {
            primary: {
              DEFAULT: "#ffffff",
              foreground: "#000000",
            },
            secondary: {
              DEFAULT: "#fdc42f",
              foreground: "#000000",
            },
            success: {
              DEFAULT: "#309443",
              foreground: "#ffffff",
            },
          },
        },
      },
    }),
  ],
};
