import { languageRedirect } from './src/locales.ts';

export default {
  async fetch(request, env) {
    const target = languageRedirect(new URL(request.url), request.headers.get('Accept-Language'));
    if (target) return new Response(null, {
      status: 302,
      headers: {
        Location: target.href,
        Vary: 'Accept-Language',
        'Cache-Control': 'no-store',
      },
    });
    return env.ASSETS.fetch(request);
  },
} satisfies ExportedHandler<Env>;
