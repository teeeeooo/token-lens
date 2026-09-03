'use strict';

const upstreamBuild = require('../../package.json').build || {};
const upstreamWin = upstreamBuild.win || {};
const {
  signtoolOptions: _upstreamSigning,
  verifyUpdateCodeSignature: _upstreamUpdateSignatureVerification,
  ...winWithoutUpstreamSigning
} = upstreamWin;

// Token Lens intentionally has a distinct Windows application identity from the
// public upstream app. Downstream artifacts are currently unsigned; do not
// inherit the upstream SignPath publisher declaration because the private fork
// does not possess that signing identity.
module.exports = {
  ...upstreamBuild,
  appId: 'com.teeeeooo.tokenlens',
  productName: 'Token Lens',
  win: {
    ...winWithoutUpstreamSigning,
    target: [
      { target: 'nsis', arch: ['x64'] },
      { target: 'portable', arch: ['x64'] }
    ]
  },
  nsis: {
    ...(upstreamBuild.nsis || {}),
    artifactName: 'Token-Lens-Setup-${version}.${ext}'
  },
  portable: {
    ...(upstreamBuild.portable || {}),
    artifactName: 'Token-Lens-${version}.${ext}'
  },
  // No GitHub publisher or updater metadata is inherited from Javis603.
  publish: null
};
