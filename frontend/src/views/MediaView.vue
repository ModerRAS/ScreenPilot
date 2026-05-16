<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Delete, EditPen, Refresh, UploadFilled } from '@element-plus/icons-vue'
import { useAppStore } from '@/stores/app'
import * as api from '@/api'
import type { MediaFileInfo } from '@/types'

type UploadStatus = 'queued' | 'uploading' | 'done' | 'error'

interface UploadItem {
  id: string
  file: File
  name: string
  size: number
  loaded: number
  total: number | null
  percent: number
  status: UploadStatus
  error: string | null
}

const store = useAppStore()
const fileInput = ref<HTMLInputElement | null>(null)
const loading = ref(false)
const isDragging = ref(false)
const uploadQueue = ref<UploadItem[]>([])

let isProcessingQueue = false
let uploadIdSequence = 0

const totalMediaSize = computed(() =>
  store.mediaFileDetails.reduce((total, file) => total + file.size, 0),
)

const activeUploads = computed(() =>
  uploadQueue.value.filter(item => item.status === 'queued' || item.status === 'uploading'),
)

onMounted(async () => {
  await refreshFiles()
})

async function refreshFiles() {
  loading.value = true
  try {
    await store.loadMediaFiles()
  } catch (e: any) {
    ElMessage.error(e.response?.data?.error ?? 'Failed to load media files')
  } finally {
    loading.value = false
  }
}

function openFilePicker() {
  fileInput.value?.click()
}

async function handleFileInput(event: Event) {
  const target = event.target as HTMLInputElement
  if (target.files?.length) {
    await enqueueFiles(Array.from(target.files))
  }
  target.value = ''
}

async function handleDrop(event: DragEvent) {
  isDragging.value = false
  const files = Array.from(event.dataTransfer?.files ?? [])
  if (files.length) {
    await enqueueFiles(files)
  }
}

async function enqueueFiles(files: File[]) {
  for (const file of files) {
    uploadQueue.value.push({
      id: createUploadId(file),
      file,
      name: file.name,
      size: file.size,
      loaded: 0,
      total: file.size || null,
      percent: 0,
      status: 'queued',
      error: null,
    })
  }

  await processUploadQueue()
}

function createUploadId(file: File): string {
  uploadIdSequence += 1
  return `${file.name}-${file.size}-${file.lastModified}-${Date.now()}-${uploadIdSequence}-${randomIdPart()}`
}

function randomIdPart(): string {
  const cryptoObject = globalThis.crypto

  if (typeof cryptoObject?.randomUUID === 'function') {
    return cryptoObject.randomUUID()
  }

  if (typeof cryptoObject?.getRandomValues === 'function') {
    const values = new Uint32Array(2)
    cryptoObject.getRandomValues(values)
    return Array.from(values, value => value.toString(36)).join('')
  }

  return Math.random().toString(36).slice(2)
}

async function processUploadQueue() {
  if (isProcessingQueue) return
  isProcessingQueue = true

  try {
    for (const item of uploadQueue.value) {
      if (item.status !== 'queued') continue

      item.status = 'uploading'
      item.error = null

      try {
        await api.uploadMedia(item.file, progress => {
          item.loaded = progress.loaded
          item.total = progress.total ?? item.size
          item.percent = progress.percent ?? 0
        })
        item.loaded = item.size
        item.total = item.size
        item.percent = 100
        item.status = 'done'
        await store.loadMediaFiles()
      } catch (e: any) {
        item.status = 'error'
        item.error = e.response?.data?.error ?? e.message ?? 'Upload failed'
      }
    }
  } finally {
    isProcessingQueue = false
  }
}

function clearFinishedUploads() {
  uploadQueue.value = uploadQueue.value.filter(item => item.status === 'uploading')
}

async function renameFile(file: MediaFileInfo) {
  try {
    const { value } = await ElMessageBox.prompt('', 'Rename File', {
      confirmButtonText: 'Save',
      cancelButtonText: 'Cancel',
      inputValue: file.name,
      inputValidator: value => validateMediaName(value) || 'Use a plain media filename.',
    })

    const newName = value.trim()
    if (newName === file.name) return

    await api.renameMediaFile(file.name, newName)
    await store.loadMediaFiles()
    ElMessage.success('File renamed')
  } catch (e: any) {
    if (e !== 'cancel' && e !== 'close') {
      ElMessage.error(e.response?.data?.error ?? e.message ?? 'Rename failed')
    }
  }
}

async function deleteFile(file: MediaFileInfo) {
  try {
    await ElMessageBox.confirm(`Delete "${file.name}"?`, 'Delete File', {
      type: 'warning',
      confirmButtonText: 'Delete',
      cancelButtonText: 'Cancel',
    })
    await api.deleteMediaFile(file.name)
    await store.loadMediaFiles()
    ElMessage.success('File deleted')
  } catch (e: any) {
    if (e !== 'cancel' && e !== 'close') {
      ElMessage.error(e.response?.data?.error ?? e.message ?? 'Delete failed')
    }
  }
}

function validateMediaName(value: string): boolean {
  const name = value.trim()
  if (!name || name.startsWith('.')) return false
  if (name.includes('/') || name.includes('\\') || name.includes('..') || name.includes('\0')) {
    return false
  }
  return /\.(mp4|webm|avi|mkv|mov)$/i.test(name)
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`
}

function formatDate(timestamp: number | null): string {
  if (!timestamp) return '-'
  return new Date(timestamp * 1000).toLocaleString()
}
</script>

<template>
  <div class="media-view">
    <div class="media-toolbar">
      <div class="media-summary">
        <h2>Media</h2>
        <span>{{ store.mediaFileDetails.length }} files · {{ formatBytes(totalMediaSize) }}</span>
      </div>
      <div class="media-actions">
        <input
          ref="fileInput"
          type="file"
          multiple
          accept=".mp4,.webm,.avi,.mkv,.mov"
          class="hidden-input"
          @change="handleFileInput"
        >
        <el-tooltip content="Upload files" placement="bottom">
          <el-button :icon="UploadFilled" type="primary" size="large" @click="openFilePicker">
            Upload
          </el-button>
        </el-tooltip>
        <el-tooltip content="Refresh" placement="bottom">
          <el-button :icon="Refresh" circle size="large" :loading="loading" @click="refreshFiles" />
        </el-tooltip>
      </div>
    </div>

    <div
      class="drop-zone"
      :class="{ 'is-dragging': isDragging }"
      @click="openFilePicker"
      @dragenter.prevent="isDragging = true"
      @dragover.prevent="isDragging = true"
      @dragleave.prevent="isDragging = false"
      @drop.prevent="handleDrop"
    >
      <el-icon><UploadFilled /></el-icon>
      <span>Drop media files here</span>
    </div>

    <div v-if="uploadQueue.length" class="upload-queue">
      <div class="queue-header">
        <strong>Uploads</strong>
        <el-button
          size="small"
          text
          :disabled="activeUploads.length > 0"
          @click="clearFinishedUploads"
        >
          Clear
        </el-button>
      </div>
      <div v-for="item in uploadQueue" :key="item.id" class="upload-item">
        <div class="upload-item-title">
          <span>{{ item.name }}</span>
          <small>{{ formatBytes(item.loaded) }} / {{ formatBytes(item.total ?? item.size) }}</small>
        </div>
        <el-progress
          :percentage="item.percent"
          :status="item.status === 'error' ? 'exception' : item.status === 'done' ? 'success' : undefined"
        />
        <div v-if="item.error" class="upload-error">{{ item.error }}</div>
      </div>
    </div>

    <el-table
      v-loading="loading"
      :data="store.mediaFileDetails"
      row-key="name"
      class="media-table"
      empty-text="No media files"
    >
      <el-table-column prop="name" label="Name" min-width="260" show-overflow-tooltip />
      <el-table-column label="Size" width="120">
        <template #default="{ row }">
          {{ formatBytes(row.size) }}
        </template>
      </el-table-column>
      <el-table-column label="Modified" min-width="180">
        <template #default="{ row }">
          {{ formatDate(row.modified) }}
        </template>
      </el-table-column>
      <el-table-column prop="extension" label="Type" width="90" />
      <el-table-column label="Actions" width="120" align="right">
        <template #default="{ row }">
          <el-tooltip content="Rename" placement="top">
            <el-button :icon="EditPen" circle text aria-label="Rename file" @click="renameFile(row)" />
          </el-tooltip>
          <el-tooltip content="Delete" placement="top">
            <el-button
              :icon="Delete"
              circle
              text
              type="danger"
              aria-label="Delete file"
              @click="deleteFile(row)"
            />
          </el-tooltip>
        </template>
      </el-table-column>
    </el-table>
  </div>
</template>

<style scoped>
.media-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.media-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.media-summary h2 {
  margin: 0;
  font-size: 22px;
}

.media-summary span {
  display: block;
  margin-top: 4px;
  color: var(--el-text-color-secondary);
}

.media-actions {
  display: flex;
  gap: 8px;
}

.hidden-input {
  display: none;
}

.drop-zone {
  min-height: 120px;
  border: 1px dashed var(--el-border-color);
  border-radius: 8px;
  display: grid;
  place-items: center;
  gap: 8px;
  color: var(--el-text-color-secondary);
  cursor: pointer;
  background: var(--el-fill-color-lighter);
}

.drop-zone :deep(.el-icon) {
  font-size: 28px;
}

.drop-zone.is-dragging {
  border-color: var(--el-color-primary);
  background: var(--el-color-primary-light-9);
  color: var(--el-color-primary);
}

.upload-queue {
  border: 1px solid var(--el-border-color);
  border-radius: 8px;
  padding: 12px;
}

.queue-header,
.upload-item-title {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: center;
}

.upload-item + .upload-item {
  margin-top: 12px;
}

.upload-item-title span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.upload-item-title small,
.upload-error {
  color: var(--el-text-color-secondary);
}

.upload-error {
  margin-top: 4px;
  color: var(--el-color-danger);
}

.media-table {
  width: 100%;
}

@media (max-width: 767px) {
  .media-actions {
    width: 100%;
  }

  .media-actions .el-button:first-of-type {
    flex: 1;
  }
}
</style>
