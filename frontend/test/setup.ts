import { config } from '@vue/test-utils'

config.global.renderStubDefaultSlot = true
config.global.config.compilerOptions ??= {}
config.global.config.compilerOptions.isCustomElement = tag => tag.startsWith('el-')

const elementPlusStubs = [
  'el-aside',
  'el-button',
  'el-col',
  'el-container',
  'el-drawer',
  'el-empty',
  'el-footer',
  'el-form',
  'el-form-item',
  'el-header',
  'el-icon',
  'el-input',
  'el-main',
  'el-menu',
  'el-menu-item',
  'el-row',
  'el-skeleton',
  'el-tag',
  'el-tooltip',
]

for (const componentName of elementPlusStubs) {
  config.global.stubs[componentName] = true
}
