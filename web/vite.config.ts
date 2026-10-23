import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    // Reachable from phones on the local network.
    host: true,
    // The match server (cargo run -p server), on the page's own origin so
    // the client needs no address for it in development.
    proxy: {
      '/ws': { target: 'ws://localhost:3000', ws: true },
    },
  },
})
