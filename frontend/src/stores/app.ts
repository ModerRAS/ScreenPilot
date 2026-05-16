import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { MediaFileInfo, RendererDevice, Scene } from '@/types'
import * as api from '@/api'

export const useAppStore = defineStore('app', () => {
  const devices = ref<RendererDevice[]>([])
  const mediaFiles = ref<string[]>([])
  const mediaFileDetails = ref<MediaFileInfo[]>([])
  const scenes = ref<Scene[]>([])
  const mediaServerUrl = ref('')
  const encoder = ref('auto')
  const isDiscovering = ref(false)
  const authChecked = ref(false)
  const isAuthenticated = ref(false)
  const authUser = ref<string | null>(null)

  function applyAuthStatus(status: { authenticated: boolean; username: string | null }) {
    isAuthenticated.value = status.authenticated
    authUser.value = status.username
  }

  async function checkAuth() {
    const status = await api.getAuthStatus()
    applyAuthStatus(status)
    authChecked.value = true
    return status
  }

  async function login(username: string, password: string) {
    const status = await api.login(username, password)
    applyAuthStatus(status)
    authChecked.value = true
    return status
  }

  async function logout() {
    const status = await api.logout()
    applyAuthStatus(status)
    devices.value = []
    mediaFiles.value = []
    mediaFileDetails.value = []
    scenes.value = []
    mediaServerUrl.value = ''
    return status
  }

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
    mediaFileDetails.value = await api.listMediaFiles()
    mediaFiles.value = mediaFileDetails.value.map(file => file.name)
  }

  async function loadScenes() {
    scenes.value = await api.getScenes()
  }

  async function loadMediaServerUrl() {
    mediaServerUrl.value = await api.getMediaServerUrl()
  }

  async function loadEncoder() {
    encoder.value = await api.getEncoder()
  }

  async function setEncoder(value: string) {
    await api.setEncoder(value)
    encoder.value = value
  }

  async function setDeviceAlias(uuid: string, alias: string | null) {
    const updated = await api.setDeviceAlias(uuid, alias)
    const index = devices.value.findIndex(device => device.uuid === uuid)
    if (index >= 0) {
      devices.value[index] = updated
    }
    return updated
  }

  return {
    devices,
    mediaFiles,
    mediaFileDetails,
    scenes,
    mediaServerUrl,
    encoder,
    isDiscovering,
    authChecked,
    isAuthenticated,
    authUser,
    checkAuth,
    login,
    logout,
    loadDevices,
    discoverDevices,
    loadMediaFiles,
    loadScenes,
    loadMediaServerUrl,
    loadEncoder,
    setEncoder,
    setDeviceAlias,
  }
})
