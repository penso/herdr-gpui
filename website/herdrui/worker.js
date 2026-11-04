// herdrui.dev after the rename. The install scripts and llms.txt keep working;
// every other path gets the notice, which forwards to the same path on sheprd.org.
const KEEP = new Set(['/install.sh', '/install.ps1', '/llms.txt']);

export default {
  fetch(request, env) {
    const url = new URL(request.url);
    if (url.hostname === 'www.herdrui.dev') {
      url.hostname = 'herdrui.dev';
      return Response.redirect(url.toString(), 301);
    }
    if (KEEP.has(url.pathname)) return env.ASSETS.fetch(request);
    url.pathname = '/';
    url.search = '';
    return env.ASSETS.fetch(new Request(url, request));
  },
};
