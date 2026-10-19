// ejmlabs site analytics: Cloudflare Web Analytics, loaded the same way on every ejmlabs site.
// Cookieless and first-party friendly. It is skipped for visitors who send Do Not Track or Global Privacy Control,
// and on any host that is not listed below (localhost, previews, workers.dev).
// To add a site: create it in Cloudflare (Analytics and Logs, Web Analytics, or POST /accounts/{id}/rum/site_info
// with auto_install false), then add its host and site token here. Tokens are public by design.
(function () {
  var SITES = {
    'ejmlabs.com': '1ccb35c8ea484d13beb469e289e0adb8',
    'cli.ejmlabs.com': '5815d1d0cf10429ab3ac683193235314',
    'ide.ejmlabs.com': '3c5bc3f589384a77801bbe33c09a95a8',
    'harness.ejmlabs.com': '0bfa5a59716e48008f6b02a8e19389f4'
  };
  var token = SITES[location.hostname];
  var dnt = navigator.doNotTrack === '1' || window.doNotTrack === '1' || navigator.globalPrivacyControl === true;
  if (!token || dnt || document.querySelector('script[data-cf-beacon]')) return;
  var s = document.createElement('script');
  s.defer = true;
  s.src = 'https://static.cloudflareinsights.com/beacon.min.js';
  s.setAttribute('data-cf-beacon', JSON.stringify({ token: token }));
  document.head.appendChild(s);
})();
