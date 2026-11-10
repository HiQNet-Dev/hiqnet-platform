import { generateKeyPairSync, randomUUID, sign } from 'node:crypto';

export type SyntheticPos = {
  installationId: string;
  deviceLabel: string;
  publicKey: string;
  privateKey: ReturnType<typeof generateKeyPairSync>['privateKey'];
};

export type EnrollmentChallenge = {
  device_code: string;
  user_code: string;
  verification_uri: string;
  nonce: string;
  expires_in: number;
};

const API_BASE = process.env.PLATFORM_API_URL ?? 'http://127.0.0.1:8080';
const PRODUCT_KEY = process.env.PLATFORM_ENROLLMENT_PRODUCT_KEY ?? 'pos';

export function createSyntheticPos(): SyntheticPos {
  const { publicKey, privateKey } = generateKeyPairSync('ed25519');
  const spki = publicKey.export({ type: 'spki', format: 'der' });
  const raw = spki.subarray(spki.length - 32);
  return {
    installationId: randomUUID(),
    deviceLabel: `POS synthetic ${randomUUID().slice(0, 8)}`,
    publicKey: `ed25519:v1:${raw.toString('base64url')}`,
    privateKey,
  };
}

export async function beginSyntheticEnrollment(pos: SyntheticPos): Promise<EnrollmentChallenge> {
  const response = await fetch(`${API_BASE}/api/v1/enrollment-requests`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      installation_id: pos.installationId,
      product_key: PRODUCT_KEY,
      public_key: pos.publicKey,
      device_label: pos.deviceLabel,
      app_version: '0.1.0-e2e',
      platform: 'linux',
    }),
  });
  if (!response.ok) {
    throw new Error(`begin enrollment falló con ${response.status}`);
  }
  return (await response.json()) as EnrollmentChallenge;
}

export function canonicalProofMessage(pos: SyntheticPos, challenge: EnrollmentChallenge): Buffer {
  return Buffer.from(
    [
      'hiqnet-enrollment-proof:v1',
      `request=${challenge.device_code}`,
      `installation=${pos.installationId}`,
      `key=${pos.publicKey}`,
      `nonce=${challenge.nonce}`,
    ].join('\n'),
    'utf8',
  );
}

export async function completeSyntheticProof(
  pos: SyntheticPos,
  challenge: EnrollmentChallenge,
): Promise<{ installation_id: string; status: string }> {
  const signature = sign(null, canonicalProofMessage(pos, challenge), pos.privateKey);
  const response = await fetch(
    `${API_BASE}/api/v1/enrollment-requests/${challenge.device_code}/proof`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        installation_id: pos.installationId,
        public_key: pos.publicKey,
        nonce: challenge.nonce,
        signature: signature.toString('base64url'),
      }),
    },
  );
  if (!response.ok) {
    throw new Error(`proof falló con ${response.status}`);
  }
  return (await response.json()) as { installation_id: string; status: string };
}
