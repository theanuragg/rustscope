import type { Config } from "tailwindcss";

const config: Config = {
  content: [
    "./pages/**/*.{js,ts,jsx,tsx,mdx}",
    "./components/**/*.{js,ts,jsx,tsx,mdx}",
    "./app/**/*.{js,ts,jsx,tsx,mdx}",
  ],
  theme: {
    extend: {
      fontFamily: {
        mono: ["'JetBrains Mono'", "monospace"],
        sans: ["'DM Sans'", "sans-serif"],
      },
      colors: {
        rust: {
          50:  "#fff7f0",
          100: "#ffe8d0",
          200: "#ffc89a",
          400: "#f07020",
          600: "#c04a00",
          800: "#7a2e00",
          900: "#4a1a00",
        },
        ice: {
          50:  "#f0f6ff",
          100: "#d6e8ff",
          200: "#a8ccf8",
          400: "#3a88e8",
          600: "#1458b0",
          800: "#0a3070",
          900: "#061840",
        },
      },
    },
  },
  plugins: [],
};
export default config;
