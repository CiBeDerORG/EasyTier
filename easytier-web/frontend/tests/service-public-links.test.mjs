import assert from 'node:assert/strict';
import { test } from 'node:test';
import { publicServiceUrl, serviceAccessLinks } from '../src/modules/service-utils.ts';

function service(overrides = {}) {
    return {
        access_url: 'https://10.144.0.5:8123/app?tab=main#status',
        publication: {
            machine_id: 'server-1', instance_id: 'network-1',
            public_ip: '203.0.113.9', bind_ip: '0.0.0.0',
            bind_port: 18443, dst_port: 8123,
            protocols: ['tcp'], server_name: 'server', network_name: 'network', dst_ip: '10.144.0.5',
        },
        ...overrides,
    };
}

test('published HTTP service keeps private link and adds mapped public link', () => {
    assert.deepEqual(serviceAccessLinks(service()), [
        { kind: 'internal', url: 'https://10.144.0.5:8123/app?tab=main#status' },
        { kind: 'public', url: 'https://203.0.113.9:18443/app?tab=main#status' },
    ]);
});

test('IPv6 public addresses are bracketed for click-through access', () => {
    assert.equal(publicServiceUrl(service({ publication: { ...service().publication, public_ip: '2001:db8::5' } })),
        'https://[2001:db8::5]:18443/app?tab=main#status');
});

test('mapping removed, missing public IP or UDP-only publication does not expose a browser public link', () => {
    const saved = service();
    assert.equal(publicServiceUrl(service({ publication: null })), null);
    assert.equal(publicServiceUrl(service({ publication: { ...saved.publication, public_ip: null } })), null);
    assert.equal(publicServiceUrl(service({ publication: { ...saved.publication, protocols: ['udp'] } })), null);
    assert.equal(publicServiceUrl(service({ publication: { ...saved.publication, bind_port: 0 } })), null);
});

test('non-HTTP or missing service URL cannot become a public web link', () => {
    assert.deepEqual(serviceAccessLinks(service({ access_url: null })), []);
    assert.deepEqual(serviceAccessLinks(service({ access_url: 'tcp://10.144.0.5:8123' })), []);
    assert.deepEqual(serviceAccessLinks(service({ access_url: 'https://user:password@10.144.0.5' })), []);
});

test('unsafe public host metadata cannot create a misleading destination', () => {
    assert.equal(publicServiceUrl(service({ publication: { ...service().publication, public_ip: 'bad/path' } })), null);
    assert.equal(publicServiceUrl(service({ publication: { ...service().publication, public_ip: 'user@host' } })), null);
});
