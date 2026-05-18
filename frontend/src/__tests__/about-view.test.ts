import { shallowMount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import AboutView from '@/views/AboutView.vue'
import { buildInfo, displayVersion, formatBuildDate } from '@/version'

const cardStub = {
  template: `
    <section class="card-stub">
      <header><slot name="header" /></header>
      <div><slot /></div>
    </section>
  `,
}

describe('AboutView', () => {
  it('renders the build metadata and service overview', () => {
    const wrapper = shallowMount(AboutView, {
      global: {
        stubs: {
          'el-card': cardStub,
        },
      },
    })

    const expectedBuildNumber = buildInfo.buildNumber || 'Local build'
    const expectedTarget = buildInfo.target || 'Local platform'
    const expectedCommit = buildInfo.gitSha || 'Unknown'

    expect(wrapper.text()).toContain('About ScreenPilot')
    expect(wrapper.text()).toContain(displayVersion)
    expect(wrapper.text()).toContain(`v${buildInfo.version}`)
    expect(wrapper.text()).toContain(buildInfo.channel || 'dev')
    expect(wrapper.text()).toContain(expectedBuildNumber)
    expect(wrapper.text()).toContain(expectedCommit)
    expect(wrapper.text()).toContain(formatBuildDate(buildInfo.buildDate))
    expect(wrapper.text()).toContain(expectedTarget)
    expect(wrapper.text()).toContain('Port 8080')
    expect(wrapper.text()).toContain('Port 8090')
    expect(wrapper.text()).toContain('Runtime Scope')
    expect(wrapper.text()).toContain('Service Layout')
  })
})
