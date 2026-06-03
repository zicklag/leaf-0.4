import adapter from '@sveltejs/adapter-static';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  vitePlugin: {
    inspector: true,
  },
  kit: {
    adapter: adapter({
      fallback: 'index.html',
    }),
  },
};

export default config;
