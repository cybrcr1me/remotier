<script setup lang="ts">
import { Button } from '@/components/ui/button'
import { Field, FieldLabel } from '@/components/ui/field'
import { COLORS, HOST_ICONS, colorSwatch } from '@/lib/appearance'
import { cn } from '@/lib/utils'
import { CheckIcon } from '@lucide/vue'

const icon = defineModel<string>('icon', { required: true })
const color = defineModel<string>('color', { required: true })

defineProps<{ showIcon?: boolean }>()
</script>

<template>
  <Field v-if="showIcon !== false">
    <FieldLabel>Icon</FieldLabel>
    <div class="flex flex-wrap gap-1">
      <Button
        v-for="option in HOST_ICONS"
        :key="option.key"
        type="button"
        variant="outline"
        size="icon"
        :aria-label="option.label"
        :aria-pressed="icon === option.key"
        :class="cn('size-9', icon === option.key && 'border-ring bg-accent')"
        @click="icon = option.key"
      >
        <component :is="option.icon" />
      </Button>
    </div>
  </Field>

  <Field>
    <FieldLabel>Colour</FieldLabel>
    <div class="flex flex-wrap gap-1">
      <Button
        v-for="option in COLORS"
        :key="option.key"
        type="button"
        variant="outline"
        size="icon"
        :aria-label="option.label"
        :aria-pressed="color === option.key"
        :class="cn('size-9', color === option.key && 'border-ring')"
        @click="color = option.key"
      >
        <span :class="cn('flex size-4 items-center justify-center rounded-full', colorSwatch(option.key))">
          <CheckIcon v-if="color === option.key" class="size-3 text-background" />
        </span>
      </Button>
    </div>
  </Field>
</template>
