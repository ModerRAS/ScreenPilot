import axios from 'axios'
import type {
  AuthStatus,
  MediaFileInfo,
  RendererDevice,
  Scene,
  SceneApplyResult,
  UploadMediaResponse,
} from '@/types'

const api = axios.create({
  baseURL: '/api',
  timeout: 60000,
})

export async function getDevices(): Promise<RendererDevice[]> {
  const { data } = await api.get<RendererDevice[]>('/devices')
  return data
}

export async function login(username: string, password: string): Promise<AuthStatus> {
  const { data } = await api.post<AuthStatus>('/auth/login', { username, password })
  return data
}

export async function logout(): Promise<AuthStatus> {
  const { data } = await api.post<AuthStatus>('/auth/logout')
  return data
}

export async function getAuthStatus(): Promise<AuthStatus> {
  const { data } = await api.get<AuthStatus>('/auth/status')
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

export async function setDeviceAlias(uuid: string, alias: string | null): Promise<RendererDevice> {
  const { data } = await api.put<RendererDevice>(`/devices/${encodeURIComponent(uuid)}/alias`, { alias })
  return data
}

export async function listMedia(): Promise<string[]> {
  const { data } = await api.get<string[]>('/media')
  return data
}

export async function listMediaFiles(): Promise<MediaFileInfo[]> {
  const { data } = await api.get<MediaFileInfo[]>('/media/files')
  return data
}

export async function uploadMedia(
  file: File,
  onProgress?: (progress: { loaded: number; total: number | null; percent: number | null }) => void,
): Promise<UploadMediaResponse> {
  const formData = new FormData()
  formData.append('file', file)
  const { data } = await api.post<UploadMediaResponse>('/media/upload', formData, {
    timeout: 0,
    onUploadProgress: event => {
      const total = event.total ?? null
      onProgress?.({
        loaded: event.loaded,
        total,
        percent: total ? Math.round((event.loaded / total) * 100) : null,
      })
    },
  })
  return data
}

export async function deleteMediaFile(filename: string): Promise<void> {
  await api.delete(`/media/files/${encodeURIComponent(filename)}`)
}

export async function renameMediaFile(filename: string, newName: string): Promise<MediaFileInfo> {
  const { data } = await api.put<MediaFileInfo>(
    `/media/files/${encodeURIComponent(filename)}/rename`,
    { new_name: newName },
  )
  return data
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

export async function getEncoder(): Promise<string> {
  const { data } = await api.get<string>('/config/encoder')
  return data
}

export async function setEncoder(encoder: string): Promise<void> {
  await api.put('/config/encoder', { encoder })
}

export async function getDeviceLoop(uuid: string): Promise<boolean> {
  const { data } = await api.get<boolean>(`/devices/${encodeURIComponent(uuid)}/loop`)
  return data
}

export async function setDeviceLoop(uuid: string, loop: boolean): Promise<void> {
  await api.put(`/devices/${encodeURIComponent(uuid)}/loop`, { loop_playback: loop })
}
