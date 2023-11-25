/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/handlers.rs"],
  theme: {
    extend: {
      boxShadow: {
        'left-side': '0 0 20px 0 rgba(0, 0, 0, 0.1), 0 0 6px 0 rgba(0, 0, 0, 0.25)',
      }
    },
  },
  plugins: [],
}

