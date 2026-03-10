<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { Monitor, Film } from '@element-plus/icons-vue'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

let refreshTimer: ReturnType<typeof setInterval> | null = null

onMounted(async () => {
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
})
</script>

<template>
  <el-container style="min-height: 100vh">
    <el-header style="display: flex; align-items: center; justify-content: space-between">
      <div style="display: flex; align-items: center; gap: 12px">
        <h1 style="font-size: 1.4rem; margin: 0">🖥 ScreenPilot</h1>
        <el-tag v-if="store.mediaServerUrl" type="info" size="small" effect="plain">
          {{ store.mediaServerUrl }}
        </el-tag>
      </div>
      <el-button
        type="primary"
        :loading="store.isDiscovering"
        @click="store.discoverDevices()"
      >
        🔍 Discover Devices
      </el-button>
    </el-header>

    <el-container>
      <el-aside width="200px" style="border-right: 1px solid var(--el-border-color)">
        <el-menu router default-active="/devices">
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

      <el-main>
        <RouterView />
      </el-main>
    </el-container>
  </el-container>
</template>
