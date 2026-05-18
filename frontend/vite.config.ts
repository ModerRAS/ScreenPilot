import { execSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import vueDevTools from 'vite-plugin-vue-devtools'

interface RootPackageJson {
  version?: string
}

const projectRoot = fileURLToPath(new URL('..', import.meta.url))
const rootPackage = JSON.parse(
  readFileSync(new URL('../package.json', import.meta.url), 'utf-8'),
) as RootPackageJson

function gitValue(command: string): string {
  try {
    return execSync(command, {
      cwd: projectRoot,
      encoding: 'utf-8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim()
  } catch {
    return ''
  }
}

const gitSha = process.env.SCREENPILOT_GIT_SHA || process.env.GITHUB_SHA || gitValue('git rev-parse HEAD')
const gitShortSha = process.env.SCREENPILOT_GIT_SHORT_SHA || (gitSha ? gitSha.slice(0, 7) : gitValue('git rev-parse --short HEAD'))
const buildDate = process.env.SCREENPILOT_BUILD_DATE || new Date().toISOString()

export default defineConfig({
  base: '/web/',
  define: {
    __SCREENPILOT_VERSION__: JSON.stringify(rootPackage.version ?? '0.0.0'),
    __SCREENPILOT_BUILD_CHANNEL__: JSON.stringify(process.env.SCREENPILOT_BUILD_CHANNEL || 'dev'),
    __SCREENPILOT_BUILD_NUMBER__: JSON.stringify(process.env.SCREENPILOT_BUILD_NUMBER || ''),
    __SCREENPILOT_BUILD_DATE__: JSON.stringify(buildDate),
    __SCREENPILOT_GIT_SHA__: JSON.stringify(gitSha),
    __SCREENPILOT_GIT_SHORT_SHA__: JSON.stringify(gitShortSha),
    __SCREENPILOT_TARGET__: JSON.stringify(process.env.SCREENPILOT_TARGET || ''),
  },
  plugins: [
    vue(),
    vueDevTools(),
  ],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url))
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
})
