<script setup lang="ts">
import {
  Clock,
  Connection,
  Cpu,
  Finished,
  InfoFilled,
  Monitor,
  Platform,
  Tickets,
} from '@element-plus/icons-vue'
import { buildInfo, displayVersion, formatBuildDate } from '@/version'

const details = [
  { label: 'Version', value: `v${buildInfo.version}`, icon: Tickets },
  { label: 'Channel', value: buildInfo.channel || 'dev', icon: Finished },
  { label: 'Build Number', value: buildInfo.buildNumber || 'Local build', icon: Platform },
  { label: 'Commit', value: buildInfo.gitSha || 'Unknown', icon: Connection, mono: true },
  { label: 'Built At', value: formatBuildDate(buildInfo.buildDate), icon: Clock },
  { label: 'Target', value: buildInfo.target || 'Local platform', icon: Cpu },
]

const capabilities = [
  'LAN DLNA/UPnP renderer discovery',
  'Media upload, rename, delete, and playback control',
  'Scene presets for applying media across multiple screens',
  'Loop playback and encoder selection for DLNA compatibility',
  'Password-protected web control interface',
]
</script>

<template>
  <div class="about-view">
    <section class="about-header">
      <div>
        <h2>About ScreenPilot</h2>
        <p>LAN digital signage control for DLNA and UPnP AV screens.</p>
      </div>
      <el-tag type="info" effect="plain" size="large">{{ displayVersion }}</el-tag>
    </section>

    <el-row :gutter="12">
      <el-col v-for="item in details" :key="item.label" :xs="24" :sm="12" :lg="8">
        <el-card shadow="never" class="info-card">
          <div class="info-row">
            <el-icon><component :is="item.icon" /></el-icon>
            <span>{{ item.label }}</span>
          </div>
          <strong :class="{ mono: item.mono }">{{ item.value }}</strong>
        </el-card>
      </el-col>
    </el-row>

    <el-card shadow="never" class="about-card">
      <template #header>
        <div class="section-title">
          <el-icon><InfoFilled /></el-icon>
          <span>Runtime Scope</span>
        </div>
      </template>
      <ul class="capability-list">
        <li v-for="capability in capabilities" :key="capability">{{ capability }}</li>
      </ul>
    </el-card>

    <el-card shadow="never" class="about-card">
      <template #header>
        <div class="section-title">
          <el-icon><Monitor /></el-icon>
          <span>Service Layout</span>
        </div>
      </template>
      <div class="service-grid">
        <div>
          <span>Web UI and API</span>
          <strong>Port 8080</strong>
        </div>
        <div>
          <span>Media streaming</span>
          <strong>Port 8090</strong>
        </div>
      </div>
    </el-card>
  </div>
</template>

<style scoped>
.about-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.about-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
  flex-wrap: wrap;
}

.about-header h2 {
  margin: 0;
  font-size: 22px;
}

.about-header p {
  margin: 6px 0 0;
  color: var(--el-text-color-secondary);
}

.info-card,
.about-card {
  border-radius: 8px;
}

.info-row,
.section-title {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--el-text-color-secondary);
}

.info-card strong {
  display: block;
  margin-top: 10px;
  overflow-wrap: anywhere;
}

.mono {
  font-family: ui-monospace, SFMono-Regular, Consolas, 'Liberation Mono', monospace;
  font-size: 13px;
}

.capability-list {
  margin: 0;
  padding-left: 20px;
  color: var(--el-text-color-regular);
}

.capability-list li + li {
  margin-top: 6px;
}

.service-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.service-grid div {
  padding: 12px;
  border: 1px solid var(--el-border-color);
  border-radius: 8px;
}

.service-grid span,
.service-grid strong {
  display: block;
}

.service-grid span {
  color: var(--el-text-color-secondary);
  margin-bottom: 6px;
}

@media (max-width: 767px) {
  .service-grid {
    grid-template-columns: 1fr;
  }
}
</style>
