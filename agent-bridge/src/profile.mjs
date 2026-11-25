import { readFile } from 'node:fs/promises';
import { BridgeError } from './errors.mjs';

const profileURL = new URL('../../config/service-profile.json', import.meta.url);

export function parseServiceProfile(text) {
  try {
    if (Buffer.byteLength(text) > 16_384) throw new Error();
    const profile = JSON.parse(text);
    if (!profile || typeof profile !== 'object' || Array.isArray(profile) ||
        profile.schemaVersion !== 1 || profile.serviceName !== 'OpenRemote' ||
        typeof profile.apiOrigin !== 'string' ||
        Object.keys(profile).some(key => !['schemaVersion', 'serviceName', 'apiOrigin'].includes(key))) throw new Error();
    if (profile.apiOrigin === '') return null;
    const origin = new URL(profile.apiOrigin);
    if (origin.protocol !== 'https:' || origin.origin !== profile.apiOrigin ||
        origin.username || origin.password || origin.search || origin.hash) throw new Error();
    return origin.origin;
  } catch {
    throw new BridgeError('invalid_service_profile');
  }
}

export async function loadServiceOrigin() {
  try {
    return parseServiceProfile(await readFile(profileURL, 'utf8'));
  } catch (error) {
    throw error instanceof BridgeError ? error : new BridgeError('invalid_service_profile');
  }
}
