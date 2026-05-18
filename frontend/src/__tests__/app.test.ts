import { reactive } from 'vue'
import { RouterLinkStub, flushPromises, shallowMount } from '@vue/test-utils'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from '@/App.vue'
import { displayVersion } from '@/version'

function createStore() {
  return reactive({
    devices: [],
    mediaFiles: [],
    mediaFileDetails: [],
    scenes: [],
    mediaServerUrl: 'http://127.0.0.1:8090',
    encoder: 'auto',
    isDiscovering: false,
    authChecked: true,
    isAuthenticated: true,
    authUser: 'admin',
    checkAuth: vi.fn(async () => ({ authenticated: true, username: 'admin' })),
    login: vi.fn(),
    logout: vi.fn(async () => ({ authenticated: false, username: null })),
    loadDevices: vi.fn(async () => {}),
    discoverDevices: vi.fn(async () => []),
    loadMediaFiles: vi.fn(async () => {}),
    loadScenes: vi.fn(async () => {}),
    loadMediaServerUrl: vi.fn(async () => {}),
    loadEncoder: vi.fn(async () => {}),
    setEncoder: vi.fn(async () => {}),
    setDeviceAlias: vi.fn(async () => {}),
  })
}

let store: ReturnType<typeof createStore>

vi.mock('@/stores/app', () => ({
  useAppStore: () => store,
}))

describe('App shell', () => {
  beforeEach(() => {
    store = createStore()
  })

  afterEach(() => {
    vi.clearAllMocks()
  })

  it('renders the footer version link and about navigation', async () => {
    const wrapper = shallowMount(App, {
      global: {
        mocks: {
          $route: { path: '/about' },
        },
        stubs: {
          RouterLink: RouterLinkStub,
          RouterView: { template: '<div class="router-view-stub" />' },
        },
      },
    })

    await flushPromises()

    const footerLink = wrapper.get('.version-footer')
    expect(footerLink.text()).toBe(displayVersion)
    expect(wrapper.text()).toContain('About')
    expect(wrapper.text()).toContain('Devices')
    expect(wrapper.text()).toContain('Media')
    expect(wrapper.text()).toContain('Scenes')
    expect(store.checkAuth).toHaveBeenCalledTimes(1)
    expect(store.loadMediaServerUrl).toHaveBeenCalledTimes(1)
    expect(store.loadMediaFiles).toHaveBeenCalledTimes(1)
    expect(store.loadDevices).toHaveBeenCalledTimes(1)
    expect(store.loadScenes).toHaveBeenCalledTimes(1)

    wrapper.unmount()
  })
})
