import {
  rateLimiter,
  rateLimitMiddleware,
} from '../src/middleware/rateLimiter.js';

jest.mock('../src/utils/tracing.js', () => ({
  __esModule: true,
  createSpan: jest.fn(() => ({ end: jest.fn() })),
  setSpanAttributes: jest.fn(),
  addSpanEvent: jest.fn(),
  getTraceId: jest.fn(() => 'test-trace-id'),
}));

jest.mock('../src/utils/alerting.js', () => ({
  __esModule: true,
  alertManager: { alert: jest.fn() },
}));

jest.mock('../src/services/redisService.js', () => ({
  __esModule: true,
  default: { logAnalytics: jest.fn().mockResolvedValue(undefined) },
}));

jest.mock('../src/services/rateLimitStrategies.js', () => ({
  __esModule: true,
  getStrategy: jest.fn().mockReturnValue({
    getName: jest.fn().mockReturnValue('SlidingWindowCounter'),
    check: jest.fn(),
  }),
}));

jest.mock('../src/config/index.js', () => ({
  __esModule: true,
  default: {
    rateLimit: {
      global: { max: 100, windowMs: 60000 },
      compile: { max: 10, windowMs: 60000 },
    },
    tracing: { serviceName: 'test', serviceVersion: '0.0.0', enabled: false },
  },
}));

function strategy() {
  return require('../src/services/rateLimitStrategies.js').getStrategy();
}

function makeReqRes(overrides = {}) {
  const req = {
    ip: '127.0.0.1',
    headers: {},
    originalUrl: '/test',
    socket: { remoteAddress: '127.0.0.1' },
    ...overrides,
  };
  const res = {
    _headers: {},
    set(keyOrObj, value) {
      if (typeof keyOrObj === 'object') {
        Object.assign(this._headers, keyOrObj);
      } else {
        this._headers[keyOrObj] = value;
      }
    },
    status() {
      return this;
    },
    json() {},
  };
  return { req, res };
}

beforeEach(() => {
  strategy().check.mockReset();
  require('../src/services/redisService.js').default.logAnalytics.mockResolvedValue(
    undefined
  );
});

describe('rateLimiter middleware', () => {
  it('calls next() when request is allowed', async () => {
    strategy().check.mockResolvedValue({
      allowed: true,
      current: 1,
      retryAfter: 0,
    });
    const { req, res } = makeReqRes();
    const next = jest.fn();

    await rateLimiter()(req, res, next);

    expect(next).toHaveBeenCalledWith();
    expect(res._headers['X-RateLimit-Limit']).toBe(100);
    expect(res._headers['X-RateLimit-Remaining']).toBe(99);
  });

  it('calls next(error) with 429 when limit exceeded', async () => {
    strategy().check.mockResolvedValue({
      allowed: false,
      current: 101,
      retryAfter: 30,
    });
    const { req, res } = makeReqRes();
    const next = jest.fn();

    await rateLimiter({ limit: 100, windowMs: 60000 })(req, res, next);

    expect(next).toHaveBeenCalledWith(
      expect.objectContaining({ statusCode: 429 })
    );
    expect(res._headers['Retry-After']).toBe('30');
  });

  it('fails open when redis throws', async () => {
    strategy().check.mockRejectedValue(new Error('Redis down'));
    const { req, res } = makeReqRes();
    const next = jest.fn();

    await rateLimiter()(req, res, next);

    expect(next).toHaveBeenCalledWith();
  });

  it('uses x-forwarded-for when req.ip is absent', async () => {
    strategy().check.mockResolvedValue({
      allowed: true,
      current: 1,
      retryAfter: 0,
    });
    const { req, res } = makeReqRes({
      ip: undefined,
      headers: { 'x-forwarded-for': '10.0.0.1' },
    });
    const next = jest.fn();

    await rateLimiter()(req, res, next);

    expect(strategy().check).toHaveBeenCalledWith(
      expect.anything(),
      expect.stringContaining('10.0.0.1'),
      expect.any(Number),
      expect.any(Number)
    );
    expect(next).toHaveBeenCalledWith();
  });

  it('uses api key as identifier when identifier=apiKey', async () => {
    strategy().check.mockResolvedValue({
      allowed: true,
      current: 1,
      retryAfter: 0,
    });
    const { req, res } = makeReqRes({ headers: { 'x-api-key': 'my-key' } });
    const next = jest.fn();

    await rateLimiter({ identifier: 'apiKey' })(req, res, next);

    expect(strategy().check).toHaveBeenCalledWith(
      expect.anything(),
      expect.stringContaining('my-key'),
      expect.any(Number),
      expect.any(Number)
    );
  });

  it('uses endpoint composite key when identifier=endpoint', async () => {
    strategy().check.mockResolvedValue({
      allowed: true,
      current: 1,
      retryAfter: 0,
    });
    const { req, res } = makeReqRes({ ip: '1.2.3.4', originalUrl: '/compile' });
    const next = jest.fn();

    await rateLimiter({ identifier: 'endpoint' })(req, res, next);

    expect(strategy().check).toHaveBeenCalledWith(
      expect.anything(),
      expect.stringContaining('1.2.3.4:/compile'),
      expect.any(Number),
      expect.any(Number)
    );
  });

  it('sets Retry-After from windowMs when retryAfter is 0', async () => {
    strategy().check.mockResolvedValue({
      allowed: false,
      current: 5,
      retryAfter: 0,
    });
    const { req, res } = makeReqRes();
    const next = jest.fn();

    await rateLimiter({ limit: 5, windowMs: 30000 })(req, res, next);

    expect(res._headers['Retry-After']).toBe('30');
  });
});

describe('rateLimitMiddleware factory', () => {
  it('returns middleware for a known config key', () => {
    expect(typeof rateLimitMiddleware('compile')).toBe('function');
  });

  it('falls back to global config for unknown key', () => {
    expect(typeof rateLimitMiddleware('unknown')).toBe('function');
  });

  it('throws when neither key nor global config exists', () => {
    const config = require('../src/config/index.js').default;
    const original = config.rateLimit.global;
    delete config.rateLimit.global;

    expect(() => rateLimitMiddleware('nonexistent')).toThrow(/not found/);

    config.rateLimit.global = original;
  });
});

describe('rate limit strategies', () => {
  it('getStrategy returns SlidingWindowCounter by default', () => {
    const { getStrategy: real } = jest.requireActual(
      '../src/services/rateLimitStrategies.js'
    );
    expect(real('unknown').getName()).toBe('SlidingWindowCounter');
  });

  it('getStrategy returns FixedWindow for FixedWindow', () => {
    const { getStrategy: real } = jest.requireActual(
      '../src/services/rateLimitStrategies.js'
    );
    expect(real('FixedWindow').getName()).toBe('FixedWindow');
  });

  it('getStrategy returns SlidingWindowLog for SlidingWindowLog', () => {
    const { getStrategy: real } = jest.requireActual(
      '../src/services/rateLimitStrategies.js'
    );
    expect(real('SlidingWindowLog').getName()).toBe('SlidingWindowLog');
  });
});
