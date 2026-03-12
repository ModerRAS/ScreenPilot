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
  <div class="devices-view">
    <div class="controls-row">
      <el-select 
        v-model="store.encoder" 
        @change="handleEncoderChange"
        class="encoder-select"
        placeholder="Encoder"
      >
        <el-option label="Auto (Hardware)" value="auto" />
        <el-option label="NVIDIA" value="nvidia" />
        <el-option label="AMD" value="amd" />
        <el-option label="Intel" value="intel" />
        <el-option label="Apple" value="apple" />
        <el-option label="Software (CPU)" value="software" />
      </el-select>

      <input
        ref="fileInput"
        type="file"
        accept=".mp4,.webm,.avi,.mkv,.mov"
        style="display: none"
        @change="handleFileChange"
      >
      <el-button 
        type="primary" 
        :loading="uploading" 
        class="upload-btn touch-target"
        @click="triggerUpload"
      >
        {{ uploading ? 'Uploading...' : '📤 Upload' }}
      </el-button>
    </div>

    <el-empty v-if="store.devices.length === 0" description="No devices found. Click Discover Devices to scan the network." />

    <el-row :gutter="12">
      <el-col v-for="device in store.devices" :key="device.uuid" :xs="24" :sm="12" :md="8" :lg="6">
        <el-card shadow="hover" class="device-card">
          <template #header>
            <div class="device-header">
              <div class="device-info">
                <strong class="device-name">{{ device.name }}</strong>
                <div class="device-ip">{{ device.ip }}</div>
              </div>
              <el-tag :type="statusType(device.status)" size="small">
                {{ device.status.toUpperCase() }}
              </el-tag>
            </div>
          </template>

          <div v-if="device.current_media" class="now-playing">
            Now playing: <strong>{{ device.current_media }}</strong>
          </div>

          <div class="media-select-row">
            <el-select
              v-model="selectedMedia[device.uuid]"
              placeholder="Select media"
              class="media-select"
              size="large"
            >
              <el-option
                v-for="file in store.mediaFiles"
                :key="file"
                :label="file"
                :value="file"
              />
            </el-select>
            <el-button type="primary" size="large" class="play-btn touch-target" @click="play(device.uuid)">▶</el-button>
          </div>

          <div class="control-row">
            <el-button
              size="large"
              class="control-btn touch-target"
              :disabled="device.status !== 'playing'"
              @click="pause(device.uuid)"
            >⏸</el-button>
            <el-button
              size="large"
              class="control-btn touch-target"
              :disabled="device.status === 'idle' || device.status === 'stopped'"
              @click="stop(device.uuid)"
            >⏹</el-button>
            <el-switch
              v-model="device.loop_playback"
              size="large"
              active-text="🔁"
              @change="(val: boolean) => toggleLoop(device.uuid, val)"
            />
          </div>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<style scoped>
.devices-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.controls-row {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}

.encoder-select {
  flex: 1;
  min-width: 140px;
}

.upload-btn {
  flex: 1;
  min-width: 140px;
}

.device-card {
  height: 100%;
  display: flex;
  flex-direction: column;
}

.device-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 8px;
}

.device-info {
  min-width: 0;
  flex: 1;
}

.device-name {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.device-ip {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.now-playing {
  margin-bottom: 12px;
  font-size: 13px;
  color: var(--el-text-color-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.media-select-row {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
}

.media-select {
  flex: 1;
}

.play-btn {
  min-width: 56px;
}

.control-row {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}

.control-btn {
  flex: 1;
  min-width: 56px;
}

.touch-target {
  min-height: 44px;
  min-width: 44px;
}

@media (max-width: 767px) {
  .controls-row {
    flex-direction: column;
  }
  
  .encoder-select,
  .upload-btn {
    width: 100%;
  }
  
  .media-select-row {
    flex-direction: column;
  }
  
  .play-btn {
    width: 100%;
  }
  
  .control-row {
    flex-direction: column;
  }
  
  .control-btn {
    width: 100%;
  }
}
</style>
