<script setup lang="ts">
/**
 * The icon of a host or group: one of the built-in set, or an app icon from the selfh.st
 * catalog.
 *
 * A catalog icon shows the built-in default until its image arrives, and keeps showing it
 * when the image cannot be fetched - offline, or an icon the catalog has since dropped - so
 * it never renders as an empty box.
 */
import { catalogReference, groupIcon, hostIcon } from '@/lib/appearance'
import { useIconsStore } from '@/stores/icons'
import { computed, watch } from 'vue'

const props = withDefaults(defineProps<{
  /** The stored icon key. */
  icon?: string | null
  /** Which default stands in for an unset key, or for a catalog image not loaded yet. */
  kind?: 'host' | 'group'
}>(), { icon: null, kind: 'host' })

const icons = useIconsStore()

const reference = computed(() => catalogReference(props.icon))
const src = computed(() => (reference.value ? icons.image(reference.value) : null))
const builtin = computed(() => (props.kind === 'group' ? groupIcon(props.icon) : hostIcon(props.icon)))

watch(reference, (value) => {
  if (value) icons.load(value)
}, { immediate: true })
</script>

<template>
  <!-- Not draggable: an image drags itself, which would hijack dragging the tab it sits in. -->
  <img v-if="src" :src="src" alt="" draggable="false" class="object-contain">
  <component :is="builtin" v-else />
</template>
