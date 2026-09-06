/**
 * Placeholder declarations and this machine's answers to them.
 *
 * `defs` are shared config; `values` are local and never leave the machine. The store
 * keeps them apart for the same reason the schema does.
 */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { ipc } from '@/lib/ipc'
import type { VarDef, VarDefInput, VarScope, VarValue } from '@/lib/types'

function valueKey(scope: VarScope, scopeId: string, name: string) {
  return `${scope}:${scopeId}:${name}`
}

export const useVarsStore = defineStore('vars', () => {
  const defs = ref<VarDef[]>([])
  const values = ref<VarValue[]>([])

  const valueMap = computed(
    () => new Map(values.value.map(v => [valueKey(v.scope, v.scopeId, v.name), v.value])),
  )

  function defsForScope(scope: VarScope, scopeId: string) {
    return defs.value.filter(d => d.scope === scope && d.scopeId === scopeId)
  }

  function valueFor(scope: VarScope, scopeId: string, name: string) {
    return valueMap.value.get(valueKey(scope, scopeId, name))
  }

  /** Declarations that still have no local answer and no default to fall back on. */
  function unresolvedRequired(scopes: { scope: VarScope, scopeId: string }[]) {
    return scopes.flatMap(({ scope, scopeId }) =>
      defsForScope(scope, scopeId).filter(
        def =>
          def.required
          && !valueFor(scope, scopeId, def.name)
          && !def.defaultValue,
      ),
    )
  }

  async function load() {
    const [loadedDefs, loadedValues] = await Promise.all([ipc.listVarDefs(), ipc.listVarValues()])
    defs.value = loadedDefs
    values.value = loadedValues
  }

  async function upsertDef(input: VarDefInput) {
    const def = await ipc.upsertVarDef(input)
    const index = defs.value.findIndex(
      d => d.scope === def.scope && d.scopeId === def.scopeId && d.name === def.name,
    )
    if (index === -1) defs.value.push(def)
    else defs.value[index] = def
    return def
  }

  async function deleteDef(id: string) {
    await ipc.deleteVarDef(id)
    defs.value = defs.value.filter(d => d.id !== id)
  }

  async function setValue(scope: VarScope, scopeId: string, name: string, value: string) {
    await ipc.setVarValue(scope, scopeId, name, value)
    const index = values.value.findIndex(
      v => v.scope === scope && v.scopeId === scopeId && v.name === name,
    )
    const entry: VarValue = { scope, scopeId, name, value }
    if (index === -1) values.value.push(entry)
    else values.value[index] = entry
  }

  async function clearValue(scope: VarScope, scopeId: string, name: string) {
    await ipc.clearVarValue(scope, scopeId, name)
    values.value = values.value.filter(
      v => !(v.scope === scope && v.scopeId === scopeId && v.name === name),
    )
  }

  return {
    defs,
    values,
    defsForScope,
    valueFor,
    unresolvedRequired,
    load,
    upsertDef,
    deleteDef,
    setValue,
    clearValue,
  }
})
