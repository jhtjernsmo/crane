// Each test starts with an empty pretend backend: no environments, no open
// folder and no saved history.
import { beforeEach } from 'vitest'
import * as preview from '../src/previewBackend'

beforeEach(() => {
  preview.reset()
  preview.resetCollection(null)
  preview.resetHistory()
  preview.resetBoxes()
  preview.resetStack()
})
