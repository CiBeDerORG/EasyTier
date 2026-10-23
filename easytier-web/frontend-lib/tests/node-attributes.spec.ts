import { describe, expect, it } from 'vitest'
import { buildDeviceInfo } from '../src/modules/utils'

const legacyMachine = () => ({
  client_url: 'tcp://198.51.100.10:45123',
  info: {
    hostname: 'OpenWrt',
    machine_id: { part1: 0, part2: 0, part3: 0, part4: 1 },
    running_network_instances: [],
  },
  location: { country: 'Legacy country', region: '', city: 'Legacy city' },
})

describe('network node attribute compatibility', () => {
  it('continues to accept machines from a server without the new fields', () => {
    const device = buildDeviceInfo(legacyMachine())
    expect(device.machine_id).toBe('00000000-0000-0000-0000-000000000001')
    expect(device.hostname).toBe('OpenWrt')
    expect(device.running_network_count).toBe(0)
    expect(device.site_location).toBeUndefined()
    expect(device.device_model).toBeUndefined()
    expect(device.connection_ip).toBeUndefined()
  })

  it('keeps manual, persisted IP and legacy locations separate for an offline node', () => {
    const device = buildDeviceInfo({
      ...legacyMachine(),
      online: false,
      site_location: 'Home cabinet',
      device_model: null,
      connection_ip: '198.51.100.10',
      ip_country: 'Observed country',
      ip_region: 'Observed region',
      ip_city: 'Observed city',
      ip_location_status: 'resolved',
      ip_observed_at: '2026-10-22T12:00:00Z',
    })
    expect(device.site_location).toBe('Home cabinet')
    expect(device.device_model).toBeNull()
    expect(device.ip_city).toBe('Observed city')
    expect(device.location?.city).toBe('Legacy city')
    expect(device.public_ip).toBe('tcp://198.51.100.10:45123')
    expect(device.connection_ip).toBe('198.51.100.10')
    expect(device.ip_observed_at).toBe('2026-10-22T12:00:00Z')
    expect(device.online).toBe(false)
  })
})
