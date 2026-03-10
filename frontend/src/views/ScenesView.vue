<script setup lang="ts">
import { ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { useAppStore } from '@/stores/app'
import * as api from '@/api'
import type { Scene } from '@/types'

const store = useAppStore()

const editorVisible = ref(false)
const editingName = ref('')
const sceneName = ref('')
const sceneAssignments = ref<Record<string, string>>({})

function showEditor(scene?: Scene) {
  if (scene) {
    editingName.value = scene.name
    sceneName.value = scene.name
    sceneAssignments.value = { ...scene.assignments }
  } else {
    editingName.value = ''
    sceneName.value = ''
    sceneAssignments.value = {}
  }
  editorVisible.value = true
}

function hideEditor() {
  editorVisible.value = false
}

function deviceName(uuid: string): string {
  const dev = store.devices.find(d => d.uuid === uuid)
  return dev ? dev.name : uuid.slice(0, 8) + '…'
}

async function save() {
  if (!sceneName.value.trim()) {
    ElMessage.warning('Scene name is required.')
    return
  }
  try {
    await api.saveScene(sceneName.value.trim(), sceneAssignments.value)
    await store.loadScenes()
    hideEditor()
    ElMessage.success(`Scene "${sceneName.value}" saved.`)
  } catch (e: any) {
    ElMessage.error(`Save failed: ${e.message}`)
  }
}

async function remove(name: string) {
  try {
    await ElMessageBox.confirm(`Delete scene "${name}"?`, 'Confirm', { type: 'warning' })
    await api.deleteScene(name)
    await store.loadScenes()
    ElMessage.success(`Scene "${name}" deleted.`)
  } catch (e: any) {
    if (e !== 'cancel') {
      ElMessage.error(`Delete failed: ${e.message}`)
    }
  }
}

async function apply(name: string) {
  try {
    const results = await api.applyScene(name)
    const failed = results.filter(r => !r.success)
    if (failed.length === 0) {
      ElMessage.success(`✅ Scene "${name}" applied to all devices.`)
    } else {
      ElMessage.warning(`⚠ Scene applied with ${failed.length} error(s).`)
    }
    await store.loadDevices()
  } catch (e: any) {
    ElMessage.error(`Apply failed: ${e.message}`)
  }
}
</script>

<template>
  <div>
    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px">
      <h2 style="margin: 0">Scenes</h2>
      <el-button type="primary" @click="showEditor()">＋ New Scene</el-button>
    </div>

    <el-empty v-if="store.scenes.length === 0 && !editorVisible" description="No scenes defined yet. Create a scene to control multiple screens at once." />

    <el-row :gutter="16">
      <el-col v-for="scene in store.scenes" :key="scene.name" :xs="24" :sm="12" :md="8">
        <el-card shadow="hover" style="margin-bottom: 16px">
          <template #header>
            <strong>🎬 {{ scene.name }}</strong>
          </template>

          <div v-for="(file, uuid) in scene.assignments" :key="uuid" style="font-size: 13px; margin-bottom: 4px; color: var(--el-text-color-secondary)">
            <strong>{{ deviceName(uuid) }}</strong> → {{ file }}
          </div>

          <template #footer>
            <div style="display: flex; gap: 8px">
              <el-button type="primary" size="small" @click="apply(scene.name)">▶ Apply</el-button>
              <el-button size="small" @click="showEditor(scene)">✏ Edit</el-button>
              <el-button type="danger" size="small" @click="remove(scene.name)">🗑 Delete</el-button>
            </div>
          </template>
        </el-card>
      </el-col>
    </el-row>

    <!-- Scene Editor Dialog -->
    <el-dialog v-model="editorVisible" :title="editingName ? `Edit Scene: ${editingName}` : 'New Scene'" width="500px">
      <el-form label-width="100px">
        <el-form-item label="Scene Name">
          <el-input v-model="sceneName" placeholder="e.g. Morning" />
        </el-form-item>

        <el-divider>Device Assignments</el-divider>

        <div v-if="store.devices.length === 0" style="text-align: center; color: var(--el-text-color-secondary); padding: 12px 0">
          No devices discovered yet. Run discovery first.
        </div>

        <el-form-item v-for="device in store.devices" :key="device.uuid" :label="device.name">
          <el-select
            v-model="sceneAssignments[device.uuid]"
            placeholder="— none —"
            clearable
            style="width: 100%"
          >
            <el-option
              v-for="file in store.mediaFiles"
              :key="file"
              :label="file"
              :value="file"
            />
          </el-select>
        </el-form-item>
      </el-form>

      <template #footer>
        <el-button @click="hideEditor()">Cancel</el-button>
        <el-button type="primary" @click="save()">Save Scene</el-button>
      </template>
    </el-dialog>
  </div>
</template>
