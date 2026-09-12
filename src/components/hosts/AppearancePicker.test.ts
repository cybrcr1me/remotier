import { mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { describe, expect, it, vi } from 'vitest'
import AppearancePicker from './AppearancePicker.vue'

// The picker reaches the icon store, which reaches IPC. Nothing rendered here needs it.
vi.mock('@/lib/ipc', () => ({
  ipc: { iconCatalog: vi.fn(), iconImage: vi.fn() },
  errorMessage: (e: unknown) => String(e),
}))

function render(props: Record<string, unknown> = {}) {
  return mount(AppearancePicker, {
    props: { icon: 'server', color: 'default', ...props },
    global: { plugins: [createPinia()] },
  })
}

describe('AppearancePicker', () => {
  it('shows the icon choices when nobody says otherwise', () => {
    // Vue casts an absent boolean prop to false. Left undefaulted, that hid the icon field -
    // built-in icons and the app icon search alike - in both editors.
    const wrapper = render()

    expect(wrapper.find('[aria-label="Server"]').exists()).toBe(true)
    expect(wrapper.text()).toContain('App icons')
  })

  it('hides them only when asked to', () => {
    const wrapper = render({ showIcon: false })

    expect(wrapper.find('[aria-label="Server"]').exists()).toBe(false)
    expect(wrapper.text()).not.toContain('App icons')
  })

  it('outlines the chosen icon and colour in lime, over the outline button\'s dark border', () => {
    // The outline variant's `dark:border-input` beats a plain border class on a `.dark`
    // root, which is why the choice showed no outline until the `dark:` twin was added.
    const wrapper = render({ icon: 'database', color: 'red' })

    for (const label of ['Database', 'Red']) {
      const classes = wrapper.get(`[aria-label="${label}"]`).classes()
      expect(classes, label).toContain('dark:border-primary')
      expect(classes, label).not.toContain('dark:border-input')
    }

    expect(wrapper.get('[aria-label="Server"]').classes()).not.toContain('border-primary')
    expect(wrapper.get('[aria-label="Blue"]').classes()).not.toContain('border-primary')
  })
})
