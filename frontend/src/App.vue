<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue'
import { Monitor, Film, Fold, Expand } from '@element-plus/icons-vue'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const isCollapsed = ref(false)
const drawerVisible = ref(false)
const isMobile = ref(false)

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

onMounted(async () => {
  checkMobile()
  window.addEventListener('resize', checkMobile)
  
  await Promise.all([
    store.loadMediaServerUrl(),
    store.loadMediaFiles(),
    store.loadDevices(),
    store.loadScenes(),
  ])
  refreshTimer = setInterval(() => store.loadDevices(), 30_000)
})

onUnmounted(() => {
  if (refreshTimer) clearInterval(refreshTimer)
  window.removeEventListener('resize', checkMobile)
})

const sidebarWidth = computed(() => isCollapsed.value ? '64px' : '200px')
</script>

<template>
  <el-container style="min-height: 100vh">
    <el-header style="display: flex; align-items: center; justify-content: space-between; padding: 0 16px">
      <div style="display: flex; align-items: center; gap: 12px">
        <el-button 
          :icon="isCollapsed ? Expand : Fold" 
          circle 
          size="large"
          class="touch-target"
          @click="toggleSidebar"
        />
        <div style="display: flex; flex-direction: column; gap: 2px">
          <h1 style="font-size: 1.2rem; margin: 0">🖥 ScreenPilot</h1>
          <el-tag v-if="store.mediaServerUrl" type="info" size="small" effect="plain" class="hide-mobile">
            {{ store.mediaServerUrl }}
          </el-tag>
        </div>
      </div>
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
    </el-header>

    <el-container>
      <el-aside 
        v-if="!isMobile"
        :width="sidebarWidth"
        style="border-right: 1px solid var(--el-border-color); transition: width 0.3s"
      >
        <el-menu 
          :collapse="isCollapsed" 
          router 
          default-active="/devices"
        >
          <el-menu-item index="/devices">
            <el-icon><Monitor /></el-icon>
            <span>Devices</span>
          </el-menu-item>
          <el-menu-item index="/scenes">
            <el-icon><Film /></el-icon>
            <span>Scenes</span>
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
          default-active="/devices"
          @select="closeDrawer"
        >
          <el-menu-item index="/devices">
            <el-icon><Monitor /></el-icon>
            <span>Devices</span>
          </el-menu-item>
          <el-menu-item index="/scenes">
            <el-icon><Film /></el-icon>
            <span>Scenes</span>
          </el-menu-item>
        </el-menu>
      </el-drawer>

      <el-main style="padding: 16px">
        <RouterView />
      </el-main>
    </el-container>
  </el-container>
</template>

<style scoped>
.touch-target {
  min-height: 44px;
  min-width: 44px;
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
