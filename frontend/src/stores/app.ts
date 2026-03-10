import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { RendererDevice, Scene } from '@/types'
import * as api from '@/api'

export const useAppStore = defineStore('app', () => {
  const devices = ref<RendererDevice[]>([])
  const mediaFiles = ref<string[]>([])
  const scenes = ref<Scene[]>([])
  const mediaServerUrl = ref('')
  const isDiscovering = ref(false)

  async function loadDevices() {
    devices.value = await api.getDevices()
  }

  async function discoverDevices() {
    isDiscovering.value = true
    try {
      devices.value = await api.discoverDevices()
    } finally {
      isDiscovering.value = false
    }
  }

  async function loadMediaFiles() {
    mediaFiles.value = await api.listMedia()
  }

  async function loadScenes() {
    scenes.value = await api.getScenes()
  }

  async function loadMediaServerUrl() {
    mediaServerUrl.value = await api.getMediaServerUrl()
  }

  return {
    devices,
    mediaFiles,
    scenes,
    mediaServerUrl,
    isDiscovering,
    loadDevices,
    discoverDevices,
    loadMediaFiles,
    loadScenes,
    loadMediaServerUrl,
  }
})
