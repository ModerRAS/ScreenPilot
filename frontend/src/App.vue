<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { ElMessage } from 'element-plus'
import {
  Film,
  Fold,
  Expand,
  FolderOpened,
  InfoFilled,
  Lock,
  Monitor,
  SwitchButton,
  User,
} from '@element-plus/icons-vue'
import { useAppStore } from '@/stores/app'
import { displayVersion } from '@/version'

const store = useAppStore()
const isCollapsed = ref(false)
const drawerVisible = ref(false)
const isMobile = ref(false)
const loginUsername = ref('admin')
const loginPassword = ref('')
const loginLoading = ref(false)

let refreshTimer: ReturnType<typeof setInterval> | null = null

function checkMobile() {
  isMobile.value = window.innerWidth < 768
  if (isMobile.value) {
    isCollapsed.value = true
  }
}

function toggleSidebar() {
  if (isMobile.value) {
    drawerVisible.value = !drawerVisible.value
  } else {
    isCollapsed.value = !isCollapsed.value
  }
}

function closeDrawer() {
  drawerVisible.value = false
}

async function loadAppData() {
  await Promise.all([
    store.loadMediaServerUrl(),
    store.loadMediaFiles(),
    store.loadDevices(),
    store.loadScenes(),
  ])
}

async function handleLogin() {
  const username = loginUsername.value.trim()
  if (!username || !loginPassword.value) {
    ElMessage.warning('Username and password are required.')
    return
  }

  loginLoading.value = true
  try {
    await store.login(username, loginPassword.value)
    loginPassword.value = ''
    await loadAppData()
    ElMessage.success('Signed in')
  } catch (e: any) {
    ElMessage.error(e.response?.data?.error ?? 'Sign in failed')
  } finally {
    loginLoading.value = false
  }
}

async function handleLogout() {
  try {
    await store.logout()
    loginPassword.value = ''
  } catch (e: any) {
    ElMessage.error(e.response?.data?.error ?? 'Sign out failed')
  }
}

onMounted(async () => {
  checkMobile()
  window.addEventListener('resize', checkMobile)

  try {
    await store.checkAuth()
    if (store.isAuthenticated) {
      await loadAppData()
    }
  } catch (e: any) {
    ElMessage.error(e.response?.data?.error ?? 'Failed to check sign-in status')
  }

  refreshTimer = setInterval(() => {
    if (store.isAuthenticated) {
      store.loadDevices()
    }
  }, 30_000)
})

onUnmounted(() => {
  if (refreshTimer) clearInterval(refreshTimer)
  window.removeEventListener('resize', checkMobile)
})

const sidebarWidth = computed(() => isCollapsed.value ? '64px' : '200px')
</script>

<template>
  <div v-if="!store.authChecked" class="auth-shell">
    <div class="auth-panel">
      <div class="auth-brand">
        <h1>ScreenPilot</h1>
        <span>Secure web control</span>
      </div>
      <el-skeleton :rows="4" animated />
    </div>
  </div>

  <div v-else-if="!store.isAuthenticated" class="auth-shell">
    <div class="auth-panel">
      <div class="auth-brand">
        <h1>ScreenPilot</h1>
        <span>Sign in to manage screens</span>
      </div>
      <el-form label-position="top" @submit.prevent="handleLogin">
        <el-form-item label="Username">
          <el-input
            v-model="loginUsername"
            :prefix-icon="User"
            size="large"
            autocomplete="username"
            @keyup.enter="handleLogin"
          />
        </el-form-item>
        <el-form-item label="Password">
          <el-input
            v-model="loginPassword"
            :prefix-icon="Lock"
            type="password"
            size="large"
            autocomplete="current-password"
            show-password
            @keyup.enter="handleLogin"
          />
        </el-form-item>
        <el-button
          type="primary"
          size="large"
          class="auth-submit"
          :loading="loginLoading"
          @click="handleLogin"
        >
          Sign In
        </el-button>
      </el-form>
    </div>
  </div>

  <el-container v-else class="app-shell">
    <el-header class="app-header">
      <div class="brand-row">
        <el-button 
          :icon="isCollapsed ? Expand : Fold" 
          circle 
          size="large"
          class="touch-target"
          @click="toggleSidebar"
        />
        <div class="brand-copy">
          <h1>ScreenPilot</h1>
          <el-tag v-if="store.mediaServerUrl" type="info" size="small" effect="plain" class="hide-mobile">
            {{ store.mediaServerUrl }}
          </el-tag>
        </div>
      </div>
      <div class="header-actions">
        <el-button
          type="primary"
          :loading="store.isDiscovering"
          size="large"
          class="touch-target"
          @click="store.discoverDevices()"
        >
          <span class="show-mobile">🔍</span>
          <span class="hide-mobile">🔍 Discover Devices</span>
        </el-button>
        <el-tooltip content="Sign out" placement="bottom">
          <el-button
            :icon="SwitchButton"
            circle
            size="large"
            class="touch-target"
            aria-label="Sign out"
            @click="handleLogout"
          />
        </el-tooltip>
      </div>
    </el-header>

    <el-container class="app-body">
      <el-aside 
        v-if="!isMobile"
        :width="sidebarWidth"
        style="border-right: 1px solid var(--el-border-color); transition: width 0.3s"
      >
        <el-menu 
          :collapse="isCollapsed" 
          router 
          :default-active="$route.path"
        >
          <el-menu-item index="/devices">
            <el-icon><Monitor /></el-icon>
            <span>Devices</span>
          </el-menu-item>
          <el-menu-item index="/media">
            <el-icon><FolderOpened /></el-icon>
            <span>Media</span>
          </el-menu-item>
          <el-menu-item index="/scenes">
            <el-icon><Film /></el-icon>
            <span>Scenes</span>
          </el-menu-item>
          <el-menu-item index="/about">
            <el-icon><InfoFilled /></el-icon>
            <span>About</span>
          </el-menu-item>
        </el-menu>
      </el-aside>

      <el-drawer
        v-model="drawerVisible"
        direction="ltr"
        :size="sidebarWidth"
        @close="closeDrawer"
      >
        <el-menu 
          router 
          :default-active="$route.path"
          @select="closeDrawer"
        >
          <el-menu-item index="/devices">
            <el-icon><Monitor /></el-icon>
            <span>Devices</span>
          </el-menu-item>
          <el-menu-item index="/media">
            <el-icon><FolderOpened /></el-icon>
            <span>Media</span>
          </el-menu-item>
          <el-menu-item index="/scenes">
            <el-icon><Film /></el-icon>
            <span>Scenes</span>
          </el-menu-item>
          <el-menu-item index="/about">
            <el-icon><InfoFilled /></el-icon>
            <span>About</span>
          </el-menu-item>
        </el-menu>
      </el-drawer>

      <el-main style="padding: 16px">
        <RouterView />
      </el-main>
    </el-container>

    <el-footer class="app-footer" height="32px">
      <RouterLink class="version-footer" to="/about">
        {{ displayVersion }}
      </RouterLink>
    </el-footer>
  </el-container>
</template>

<style scoped>
.auth-shell {
  min-height: 100vh;
  display: grid;
  place-items: center;
  padding: 24px;
  background: #f5f7fb;
}

.auth-panel {
  width: min(100%, 380px);
  padding: 28px;
  border: 1px solid var(--el-border-color);
  border-radius: 8px;
  background: var(--el-bg-color);
  box-shadow: 0 12px 32px rgb(31 41 55 / 12%);
}

.auth-brand {
  margin-bottom: 24px;
}

.auth-brand h1 {
  margin: 0;
  font-size: 24px;
  line-height: 1.2;
}

.auth-brand span {
  display: block;
  margin-top: 6px;
  color: var(--el-text-color-secondary);
}

.auth-submit {
  width: 100%;
}

.app-shell {
  min-height: 100vh;
}

.app-body {
  flex: 1;
  min-height: 0;
}

.app-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 16px;
}

.brand-row {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.brand-copy {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.brand-copy h1 {
  margin: 0;
  font-size: 1.2rem;
  line-height: 1.2;
}

.header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.touch-target {
  min-height: 44px;
  min-width: 44px;
}

.el-aside {
  display: flex;
  flex-direction: column;
}

.el-menu {
  border-right: 0;
}

.app-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding: 0 16px;
  border-top: 1px solid var(--el-border-color-lighter);
  background: var(--el-bg-color);
}

.version-footer {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.3;
  text-decoration: none;
  overflow-wrap: anywhere;
}

.version-footer:hover {
  color: var(--el-color-primary);
}

@media (max-width: 767px) {
  .el-header {
    height: auto !important;
    min-height: 60px;
    padding: 8px !important;
  }
  
  .el-main {
    padding: 12px !important;
  }
}
</style>
