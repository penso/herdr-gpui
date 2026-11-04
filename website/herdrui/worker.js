// herdrui.dev after the rename. The install scripts, llms.txt and social card stay;
// every other path gets the notice, which forwards to the same path on sheprd.org.
const KEEP = new Set(['/install.sh', '/install.ps1', '/llms.txt', '/assets/rename-social.png']);

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
