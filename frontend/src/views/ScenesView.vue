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
  <div class="scenes-view">
    <div class="header-row">
      <h2 style="margin: 0">Scenes</h2>
      <el-button type="primary" size="large" class="touch-target" @click="showEditor()">＋ New</el-button>
    </div>

    <el-empty v-if="store.scenes.length === 0 && !editorVisible" description="No scenes defined yet. Create a scene to control multiple screens at once." />

    <el-row :gutter="12">
      <el-col v-for="scene in store.scenes" :key="scene.name" :xs="24" :sm="12" :md="8">
        <el-card shadow="hover" class="scene-card">
          <template #header>
            <strong>🎬 {{ scene.name }}</strong>
          </template>

          <div v-for="(file, uuid) in scene.assignments" :key="uuid" class="scene-assignment">
            <strong>{{ deviceName(uuid) }}</strong> → {{ file }}
          </div>

          <template #footer>
            <div class="scene-actions">
              <el-button type="primary" size="large" class="touch-target" @click="apply(scene.name)">▶</el-button>
              <el-button size="large" class="touch-target" @click="showEditor(scene)">✏</el-button>
              <el-button type="danger" size="large" class="touch-target" @click="remove(scene.name)">🗑</el-button>
            </div>
          </template>
        </el-card>
      </el-col>
    </el-row>

    <el-dialog 
      v-model="editorVisible" 
      :title="editingName ? `Edit Scene: ${editingName}` : 'New Scene'" 
      :fullscreen="true"
      class="scene-dialog"
    >
      <div class="dialog-content">
        <el-form label-position="top">
          <el-form-item label="Scene Name">
            <el-input v-model="sceneName" placeholder="e.g. Morning" size="large" />
          </el-form-item>

          <el-divider>Device Assignments</el-divider>

          <div v-if="store.devices.length === 0" style="text-align: center; color: var(--el-text-color-secondary); padding: 24px 0">
            No devices discovered yet. Run discovery first.
          </div>

          <el-form-item v-for="device in store.devices" :key="device.uuid" :label="device.name">
            <el-select
              v-model="sceneAssignments[device.uuid]"
              placeholder="— none —"
              clearable
              size="large"
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
      </div>

      <template #footer>
        <div class="dialog-footer">
          <el-button size="large" class="touch-target" @click="hideEditor()">Cancel</el-button>
          <el-button type="primary" size="large" class="touch-target" @click="save()">Save Scene</el-button>
        </div>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
.scenes-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.header-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  flex-wrap: wrap;
  gap: 12px;
}

.scene-card {
  height: 100%;
}

.scene-assignment {
  font-size: 13px;
  margin-bottom: 4px;
  color: var(--el-text-color-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.scene-actions {
  display: flex;
  gap: 8px;
}

.scene-actions .el-button {
  flex: 1;
}

.touch-target {
  min-height: 44px;
  min-width: 44px;
}

.dialog-content {
  padding: 16px;
  max-width: 600px;
  margin: 0 auto;
}

.dialog-footer {
  display: flex;
  gap: 12px;
  justify-content: flex-end;
}

.dialog-footer .el-button {
  flex: 1;
}

@media (min-width: 768px) {
  .el-dialog {
    width: 500px !important;
    max-width: 90vw;
  }
  
  .el-dialog.is-fullscreen {
    width: 100% !important;
    max-width: 600px;
  }
  
  .dialog-content {
    padding: 0;
  }
  
  .dialog-footer {
    flex-direction: row;
  }
  
  .dialog-footer .el-button {
    flex: none;
    min-width: 100px;
  }
}
</style>
