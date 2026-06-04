import { EventEmitter } from 'events';

// Capture the connection handler set by setupWebsocketServer
let connectionHandler = null;
const wssHandlers = {};

jest.mock('ws', () => ({
  __esModule: true,
  WebSocketServer: jest.fn(() => ({
    on: jest.fn((event, handler) => {
      wssHandlers[event] = handler;
      if (event === 'connection') connectionHandler = handler;
    }),
  })),
}));

jest.mock('../src/utils/tracing.js', () => ({
  __esModule: true,
  createSpan: jest.fn(() => ({ end: jest.fn() })),
  setSpanAttributes: jest.fn(),
  addSpanEvent: jest.fn(),
  getTraceId: jest.fn(),
}));

jest.mock('../src/utils/alerting.js', () => ({
  __esModule: true,
  alertManager: { alert: jest.fn() },
}));

jest.mock('../src/config/index.js', () => ({
  __esModule: true,
  default: {
    rateLimit: {
      global: { max: 100, windowMs: 60000 },
      compile: { max: 10, windowMs: 60000 },
    },
    tracing: { serviceName: 'test', serviceVersion: '0.0.0', enabled: false },
    compile: { timeoutMs: 30000 },
  },
}));

jest.mock('../src/services/invokeService.js', () => {
  const { EventEmitter } = require('events');
  return { __esModule: true, invokeProgressBus: new EventEmitter() };
});
jest.mock('../src/services/deployService.js', () => {
  const { EventEmitter } = require('events');
  return { __esModule: true, deployProgressBus: new EventEmitter() };
});
jest.mock('../src/services/compileService.js', () => {
  const { EventEmitter } = require('events');
  return { __esModule: true, compileProgressBus: new EventEmitter() };
});
jest.mock('../src/services/oracleProofQueueService.js', () => {
  const { EventEmitter } = require('events');
  return { __esModule: true, default: new EventEmitter() };
});
jest.mock('../src/services/oracle/oracleEvents.js', () => ({
  __esModule: true,
  sharedOracleEventBus: { on: jest.fn() },
}));
jest.mock('../src/services/redisService.js', () => ({
  __esModule: true,
  default: { isFallbackMode: true, client: null },
}));

import {
  setupWebsocketServer,
  broadcast,
  broadcastTreasuryEvent,
} from '../src/websocket.js';

function makeSocket(readyState = 1) {
  const handlers = {};
  return {
    readyState,
    OPEN: 1,
    send: jest.fn(),
    close: jest.fn(),
    on: jest.fn((event, fn) => {
      handlers[event] = fn;
    }),
    _handlers: handlers,
  };
}

function makeRequest(url = '/ws', headers = {}) {
  return { url, headers };
}

beforeAll(() => {
  setupWebsocketServer({ on: jest.fn() });
});

describe('WebSocket server', () => {
  it('sends connected message on new connection', () => {
    const socket = makeSocket();
    connectionHandler(socket, makeRequest());
    expect(socket.send).toHaveBeenCalledWith(
      expect.stringContaining('"type":"connected"')
    );
  });

  it('closes socket with 1008 when URL is malformed', () => {
    const socket = makeSocket();
    connectionHandler(socket, makeRequest('http://'));
    expect(socket.close).toHaveBeenCalledWith(1008, 'Bad Request');
    expect(socket.send).not.toHaveBeenCalled();
  });

  it('closes socket with 1008 when auth token is wrong', () => {
    process.env.WS_AUTH_TOKEN = 'secret';
    const socket = makeSocket();
    connectionHandler(
      socket,
      makeRequest('/ws', { authorization: 'Bearer wrong' })
    );
    expect(socket.close).toHaveBeenCalledWith(1008, 'Unauthorized');
    delete process.env.WS_AUTH_TOKEN;
  });

  it('accepts connection when token matches via query param', () => {
    process.env.WS_AUTH_TOKEN = 'secret';
    const socket = makeSocket();
    connectionHandler(socket, makeRequest('/ws?token=secret'));
    expect(socket.send).toHaveBeenCalledWith(
      expect.stringContaining('"type":"connected"')
    );
    delete process.env.WS_AUTH_TOKEN;
  });

  it('removes client on socket close so broadcast skips it', () => {
    const socket = makeSocket();
    connectionHandler(socket, makeRequest());
    socket.send.mockClear();

    socket._handlers['close']();
    broadcast({ type: 'test' });

    expect(socket.send).not.toHaveBeenCalled();
  });

  it('removes client on socket error without throwing', () => {
    const socket = makeSocket();
    connectionHandler(socket, makeRequest());
    socket.send.mockClear();

    expect(() => socket._handlers['error'](new Error('reset'))).not.toThrow();
    broadcast({ type: 'test' });

    expect(socket.send).not.toHaveBeenCalled();
  });

  it('does not send to non-OPEN sockets', () => {
    const socket = makeSocket(3); // CLOSING
    connectionHandler(socket, makeRequest());
    expect(socket.send).not.toHaveBeenCalled();
  });

  it('broadcast sends to all open clients', () => {
    const s1 = makeSocket();
    const s2 = makeSocket();
    connectionHandler(s1, makeRequest());
    connectionHandler(s2, makeRequest());
    s1.send.mockClear();
    s2.send.mockClear();

    broadcast({ type: 'ping' });

    expect(s1.send).toHaveBeenCalledWith(
      expect.stringContaining('"type":"ping"')
    );
    expect(s2.send).toHaveBeenCalledWith(
      expect.stringContaining('"type":"ping"')
    );
  });

  it('broadcast handles non-serializable payload without throwing', () => {
    const socket = makeSocket();
    connectionHandler(socket, makeRequest());
    socket.send.mockClear();

    const circular = {};
    circular.self = circular;

    expect(() => broadcast(circular)).not.toThrow();
    expect(socket.send).not.toHaveBeenCalled();
  });

  it('broadcastTreasuryEvent sends treasury-event type to clients', () => {
    const socket = makeSocket();
    connectionHandler(socket, makeRequest());
    socket.send.mockClear();

    broadcastTreasuryEvent({ proposalId: 1, action: 'vote' });

    expect(socket.send).toHaveBeenCalledWith(
      expect.stringContaining('"type":"treasury-event"')
    );
  });
});
