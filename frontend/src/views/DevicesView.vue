<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { ElMessage } from 'element-plus'
import { useAppStore } from '@/stores/app'
import * as api from '@/api'

const store = useAppStore()
const selectedMedia = ref<Record<string, string>>({})
const uploading = ref(false)
const fileInput = ref<HTMLInputElement | null>(null)

onMounted(async () => {
  await store.loadEncoder()
})

async function triggerUpload() {
  fileInput.value?.click()
}

async function handleFileChange(event: Event) {
  const target = event.target as HTMLInputElement
  const file = target.files?.[0]
  if (!file) return

  uploading.value = true
  try {
    await api.uploadMedia(file)
    ElMessage.success(`Uploaded "${file.name}"`)
    await store.loadMediaFiles()
  } catch (e: any) {
    ElMessage.error(`Upload failed: ${e.message}`)
  } finally {
    uploading.value = false
    target.value = ''
  }
}

function statusType(status: string): '' | 'success' | 'warning' | 'info' | 'danger' {
  switch (status) {
    case 'playing': return 'success'
    case 'paused': return 'warning'
    case 'stopped': return 'info'
    case 'error': return 'danger'
    default: return 'info'
  }
}

async function play(uuid: string) {
  const filename = selectedMedia.value[uuid]
  if (!filename) {
    ElMessage.warning('Please select a media file first.')
    return
  }
  try {
    await api.playOnDevice(uuid, filename)
    ElMessage.success(`▶ Playing "${filename}"`)
    await store.loadDevices()
  } catch (e: any) {
    ElMessage.error(`Play failed: ${e.message}`)
  }
}

async function pause(uuid: string) {
  try {
    await api.pauseDevice(uuid)
    ElMessage.success('⏸ Paused')
    await store.loadDevices()
  } catch (e: any) {
    ElMessage.error(`Pause failed: ${e.message}`)
  }
}

async function stop(uuid: string) {
  try {
    await api.stopDevice(uuid)
    ElMessage.success('⏹ Stopped')
    await store.loadDevices()
  } catch (e: any) {
    ElMessage.error(`Stop failed: ${e.message}`)
  }
}

async function handleEncoderChange(value: string) {
  try {
    await store.setEncoder(value)
    ElMessage.success(`Encoder set to ${value}`)
  } catch (e: any) {
    ElMessage.error('Failed to set encoder')
  }
}

async function toggleLoop(uuid: string, loop: boolean) {
  try {
    await api.setDeviceLoop(uuid, loop)
    ElMessage.success(loop ? 'Loop enabled' : 'Loop disabled')
    await store.loadDevices()
  } catch (e: any) {
    ElMessage.error('Failed to set loop')
    await store.loadDevices()
  }
}
</script>

<template>
  <div>
    <!-- Encoder selection -->
    <div style="margin-bottom: 16px; display: flex; align-items: center; gap: 8px;">
      <span>Encoder:</span>
      <el-select v-model="store.encoder" @change="handleEncoderChange">
        <el-option label="Auto (Hardware)" value="auto" />
        <el-option label="NVIDIA" value="nvidia" />
        <el-option label="AMD" value="amd" />
        <el-option label="Intel" value="intel" />
        <el-option label="Apple" value="apple" />
        <el-option label="Software (CPU)" value="software" />
      </el-select>
    </div>

    <div style="margin-bottom: 16px">
      <input
        ref="fileInput"
        type="file"
        accept=".mp4,.webm,.avi,.mkv,.mov"
        style="display: none"
        @change="handleFileChange"
      >
      <el-button type="primary" :loading="uploading" @click="triggerUpload">
        {{ uploading ? 'Uploading...' : 'Upload Video' }}
      </el-button>
    </div>

    <el-empty v-if="store.devices.length === 0" description="No devices found. Click Discover Devices to scan the network." />

    <el-row :gutter="16">
      <el-col v-for="device in store.devices" :key="device.uuid" :xs="24" :sm="12" :md="8" :lg="6">
        <el-card shadow="hover" style="margin-bottom: 16px">
          <template #header>
            <div style="display: flex; justify-content: space-between; align-items: center">
              <div>
                <strong>{{ device.name }}</strong>
                <div style="font-size: 12px; color: var(--el-text-color-secondary)">{{ device.ip }}</div>
              </div>
              <el-tag :type="statusType(device.status)" size="small">
                {{ device.status.toUpperCase() }}
              </el-tag>
            </div>
          </template>

          <div v-if="device.current_media" style="margin-bottom: 12px; font-size: 13px; color: var(--el-text-color-secondary)">
            Now playing: <strong>{{ device.current_media }}</strong>
          </div>

          <div style="display: flex; gap: 8px; margin-bottom: 12px">
            <el-select
              v-model="selectedMedia[device.uuid]"
              placeholder="Select media"
              style="flex: 1"
              size="small"
            >
              <el-option
                v-for="file in store.mediaFiles"
                :key="file"
                :label="file"
                :value="file"
              />
            </el-select>
            <el-button type="primary" size="small" @click="play(device.uuid)">▶ Play</el-button>
          </div>

          <div style="display: flex; gap: 8px">
            <el-button
              size="small"
              :disabled="device.status !== 'playing'"
              @click="pause(device.uuid)"
            >⏸ Pause</el-button>
            <el-button
              size="small"
              :disabled="device.status === 'idle' || device.status === 'stopped'"
              @click="stop(device.uuid)"
            >⏹ Stop</el-button>
            <el-switch
              v-model="device.loop_playback"
              size="small"
              active-text="Loop"
              @change="(val: boolean) => toggleLoop(device.uuid, val)"
            />
          </div>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>
