// A stand-in for the two Microsoft services publish.ps1 talks to (the Entra token endpoint and the
// Store submission API, as Microsoft documents them), for test-publish.ps1: it answers like them,
// checks every request publish.ps1 makes, and writes what it saw to a report.
//
//   node store-standin.mjs <port> <report.json> <expected package name> <whats-new.txt>
//
// It ends by itself once the submission's status has been read twice, or after a minute.
import { readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';

const [port, reportPath, packageName, whatsNewFile] = process.argv.slice(2);
const whatsNew = readFileSync(whatsNewFile, 'utf8').replace(/^\uFEFF/, '').trim();
const APP = '9PHR0L7S1QVD';
const PENDING = '1152921505000000001';
const NEW = '1152921505000000002';
const seen = [];
const checks = {};
let statusReads = 0;

/** The last published submission, as the API copies it into a new one. */
const published = () => ({
  id: NEW,
  applicationCategory: 'Productivity',
  pricing: { trialPeriod: 'NoFreeTrial', marketSpecificPricings: {}, sales: [], priceId: 'Free', isAdvertisingFunded: false },
  visibility: 'Public',
  targetPublishMode: 'Immediate',
  targetPublishDate: '1601-01-01T00:00:00Z',
  listings: {
    'en-us': {
      baseListing: {
        copyrightAndTrademarkInfo: '',
        keywords: ['window manager', 'workspaces', 'tabs', 'zones', 'tiling'],
        licenseTerms: '',
        privacyPolicy: '',
        supportContact: '',
        websiteUrl: '',
        description: 'Mosaïk gives every project its own workspace.',
        features: ['Zones that belong to the project, not the monitor'],
        releaseNotes: '',
        images: [{ fileName: 'shot1.png', fileStatus: 'Uploaded', id: '1', description: '', imageType: 'Screenshot' }],
        recommendedHardware: [],
        title: 'Mosaïk',
      },
      platformOverrides: {},
    },
  },
  hardwarePreferences: [],
  automaticBackupEnabled: false,
  canInstallOnRemovableMedia: true,
  isGameDvrEnabled: false,
  gamingOptions: [],
  hasExternalInAppProducts: false,
  meetAccessibilityGuidelines: true,
  notesForCertification: '',
  status: 'PendingCommit',
  statusDetails: { errors: [], warnings: [], certificationReports: [] },
  fileUploadUrl: `http://127.0.0.1:${port}/upload/submission-${NEW}?sv=2026-01-01&sig=standin`,
  applicationPackages: [
    {
      fileName: 'Mosaik_1.0.0.0_x64.msix',
      fileStatus: 'Uploaded',
      id: '1152921504621465124',
      version: '1.0.0.0',
      architecture: 'X64',
      languages: ['en-us'],
      capabilities: ['runFullTrust'],
      minimumDirectXVersion: 'None',
      minimumSystemRam: 'None',
      targetDeviceFamilies: ['Windows.Desktop min version 10.0.19045.0'],
    },
  ],
  packageDeliveryOptions: { packageRollout: { isPackageRollout: false, packageRolloutPercentage: 0, packageRolloutStatus: 'PackageRolloutNotStarted', fallbackSubmissionId: '0' }, isMandatoryUpdate: false, mandatoryUpdateEffectiveDate: '1601-01-01T00:00:00.0000000Z' },
  enterpriseLicensing: 'Online',
  allowMicrosoftDecideAppAvailabilityToFutureDeviceFamilies: true,
  allowTargetFutureDeviceFamilies: { Desktop: true },
  friendlyName: 'Submission 2',
  trailers: [],
});

function finish() {
  writeFileSync(reportPath, JSON.stringify({ checks, seen }, null, 2));
  server.close();
  setTimeout(() => process.exit(0), 50);
}

const isArray = (x) => Array.isArray(x);

const server = createServer((req, res) => {
  const chunks = [];
  req.on('data', (c) => chunks.push(c));
  req.on('end', () => {
    const body = Buffer.concat(chunks);
    const url = new URL(req.url, `http://127.0.0.1:${port}`);
    const path = url.pathname;
    seen.push(`${req.method} ${path}`);
    const json = (code, value) => {
      res.writeHead(code, { 'Content-Type': 'application/json; charset=utf-8' });
      res.end(value === undefined ? '' : JSON.stringify(value));
    };
    const authorized = req.headers.authorization === 'Bearer standin-token';
    const base = `/api/applications/${APP}`;

    if (req.method === 'POST' && path === '/login/test-tenant/oauth2/token') {
      const form = new URLSearchParams(body.toString('utf8'));
      checks.token_form =
        form.get('grant_type') === 'client_credentials' &&
        form.get('client_id') === 'test-client' &&
        form.get('client_secret') === 'test-key' &&
        form.get('resource') === 'https://manage.devcenter.microsoft.com';
      return json(200, { token_type: 'Bearer', expires_in: '3599', access_token: 'standin-token' });
    }
    if (!path.startsWith('/upload/') && !authorized) {
      checks.unauthorized = `${req.method} ${path}`;
      return json(401, { code: 'Unauthorized', message: 'No token' });
    }
    if (req.method === 'GET' && path === base) {
      return json(200, {
        id: APP,
        primaryName: 'Mosaïk',
        packageFamilyName: 'Gimzdev.Mosak_yjd3d5zpwkx98',
        lastPublishedApplicationSubmission: { id: '1152921505000000000', resourceLocation: `applications/${APP}/submissions/1152921505000000000` },
        pendingApplicationSubmission: { id: PENDING, resourceLocation: `applications/${APP}/submissions/${PENDING}` },
      });
    }
    if (req.method === 'DELETE' && path === `${base}/submissions/${PENDING}`) {
      checks.pending_deleted = true;
      res.writeHead(204);
      return res.end();
    }
    if (req.method === 'POST' && path === `${base}/submissions`) {
      checks.created_after_delete = checks.pending_deleted === true;
      return json(201, published());
    }
    if (req.method === 'PUT' && path === `${base}/submissions/${NEW}`) {
      let sent;
      try {
        sent = JSON.parse(body.toString('utf8'));
      } catch {
        checks.update_json = false;
        return json(400, { code: 'InvalidJson' });
      }
      const packages = sent.applicationPackages;
      checks.update_json = true;
      checks.packages_are_an_array = isArray(packages);
      checks.old_package_removed = isArray(packages) && packages.some((p) => p.fileName === 'Mosaik_1.0.0.0_x64.msix' && p.fileStatus === 'PendingDelete');
      checks.new_package_added = isArray(packages) && packages.some((p) => p.fileName === packageName && p.fileStatus === 'PendingUpload');
      const listing = sent.listings?.['en-us']?.baseListing;
      checks.whats_new = listing?.releaseNotes === whatsNew;
      checks.listing_kept =
        listing?.title === 'Mosaïk' &&
        listing?.description === 'Mosaïk gives every project its own workspace.' &&
        isArray(listing?.keywords) && listing.keywords.length === 5 &&
        isArray(listing?.features) && isArray(listing?.images);
      checks.pricing_kept = sent.pricing?.priceId === 'Free';
      return json(200, sent);
    }
    if (req.method === 'PUT' && path === `/upload/submission-${NEW}`) {
      checks.blob_type = req.headers['x-ms-blob-type'] === 'BlockBlob';
      checks.upload_is_a_zip = body.length > 22 && body[0] === 0x50 && body[1] === 0x4b;
      checks.zip_has_the_package = body.includes(Buffer.from(packageName, 'utf8'));
      checks.upload_after_update = checks.update_json === true;
      res.writeHead(201);
      return res.end();
    }
    if (req.method === 'POST' && path === `${base}/submissions/${NEW}/commit`) {
      checks.commit_after_upload = checks.upload_is_a_zip === true;
      return json(202, { status: 'CommitStarted' });
    }
    if (req.method === 'GET' && path === `${base}/submissions/${NEW}/status`) {
      statusReads += 1;
      const status = statusReads === 1 ? 'CommitStarted' : 'PreProcessing';
      json(200, { status, statusDetails: { errors: [], warnings: [], certificationReports: [] } });
      if (statusReads >= 2) {
        checks.status_read_until_taken = true;
        finish();
      }
      return undefined;
    }
    checks.unexpected = `${req.method} ${path}`;
    return json(404, { code: 'NotFound', message: `No ${req.method} ${path} here` });
  });
});

server.listen(Number(port), '127.0.0.1');
setTimeout(finish, 60_000);
