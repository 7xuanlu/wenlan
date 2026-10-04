// SPDX-License-Identifier: Apache-2.0
// LOCAL TEST ONLY. No authentication: never deploy this transport fixture.
import { ReverseTransport } from '../../src/reverse-transport.ts';

interface Env { AUTHORITY: DurableObjectNamespace }

export class ReverseFixtureAuthority {
  private transport?: ReverseTransport;

  async fetch(request: Request): Promise<Response> {
    const path = new URL(request.url).pathname;
    if (path === '/socket' && request.headers.get('upgrade') === 'websocket') {
      this.transport?.disconnect();
      const pair = new WebSocketPair();
      const [client, server] = Object.values(pair);
      server.accept();
      const transport = new ReverseTransport({
        send: text => server.send(text),
        close: () => server.close(1000, 'Closed'),
      }, { deadlineMs: 2000 });
      server.addEventListener('message', event => transport.receive(event.data));
      server.addEventListener('close', () => transport.disconnect());
      server.addEventListener('error', () => transport.disconnect());
      this.transport = transport;
      return new Response(null, { status: 101, webSocket: client });
    }
    if (path === '/query' && this.transport) {
      try {
        return await this.transport.request({ path: '/mcp', method: 'POST', headers: {
          'content-type': 'application/json', accept: 'application/json, text/event-stream',
        }, body: btoa('{"jsonrpc":"2.0","id":1,"method":"tools/list"}') }, request.signal);
      } catch { return Response.json({ error: 'Connector unavailable' }, { status: 503 }); }
    }
    return new Response(null, { status: 404 });
  }
}

export default {
  fetch(request: Request, env: Env) {
    return env.AUTHORITY.get(env.AUTHORITY.idFromName('reverse-synthetic')).fetch(request);
  },
};
