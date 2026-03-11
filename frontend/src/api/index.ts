import axios from 'axios'
import type { RendererDevice, Scene, SceneApplyResult } from '@/types'

const api = axios.create({
  baseURL: '/api',
  timeout: 15000,
})

export async function getDevices(): Promise<RendererDevice[]> {
  const { data } = await api.get<RendererDevice[]>('/devices')
  return data
}

export async function discoverDevices(): Promise<RendererDevice[]> {
  const { data } = await api.post<RendererDevice[]>('/devices/discover')
  return data
}

export async function playOnDevice(uuid: string, mediaFilename: string): Promise<void> {
  await api.post(`/devices/${encodeURIComponent(uuid)}/play`, { media_filename: mediaFilename })
}

export async function pauseDevice(uuid: string): Promise<void> {
  await api.post(`/devices/${encodeURIComponent(uuid)}/pause`)
}

export async function stopDevice(uuid: string): Promise<void> {
  await api.post(`/devices/${encodeURIComponent(uuid)}/stop`)
}

export async function listMedia(): Promise<string[]> {
  const { data } = await api.get<string[]>('/media')
  return data
}

export async function uploadMedia(file: File): Promise<void> {
  const formData = new FormData()
  formData.append('file', file)
  await api.post('/media/upload', formData)
}

export async function getScenes(): Promise<Scene[]> {
  const { data } = await api.get<Scene[]>('/scenes')
  return data
}

export async function saveScene(name: string, assignments: Record<string, string>): Promise<void> {
  await api.post('/scenes', { name, assignments })
}

export async function deleteScene(name: string): Promise<void> {
  await api.delete(`/scenes/${encodeURIComponent(name)}`)
}

export async function applyScene(name: string): Promise<SceneApplyResult[]> {
  const { data } = await api.post<SceneApplyResult[]>(`/scenes/${encodeURIComponent(name)}/apply`)
  return data
}

export async function getMediaServerUrl(): Promise<string> {
  const { data } = await api.get<string>('/config/media-server-url')
  return data
}
