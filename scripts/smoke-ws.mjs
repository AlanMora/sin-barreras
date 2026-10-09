// Smoke test de punta a punta del protocolo v1 contra el sistema levantado
// con `docker compose --profile apps up`.
//
//   node scripts/smoke-ws.mjs [ws://localhost:8000/ws]
//
// Verifica: hello/welcome, creación de incidente, chat con ack, eco y
// estados de entrega, respuesta del operador simulado y reanudación de la
// sesión con recuperación por seq. Sale con código 1 si algo falla.

const url = process.argv[2] ?? 'ws://localhost:8000/ws';
const TIMEOUT_MS = 20_000;
const uuid = () => crypto.randomUUID();

function open(target) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(target);
    const frames = [];
    const waiters = [];
    ws.addEventListener('message', (event) => {
      const frame = JSON.parse(event.data);
      frames.push(frame);
      for (const w of [...waiters]) {
        if (w.pred(frame)) {
          waiters.splice(waiters.indexOf(w), 1);
          w.resolve(frame);
        }
      }
    });
    ws.addEventListener('open', () =>
      resolve({
        ws,
        frames,
        send: (msg) => ws.send(JSON.stringify(msg)),
        wait: (pred, what) =>
          new Promise((ok, fail) => {
            const found = frames.find(pred);
            if (found) return ok(found);
            const timer = setTimeout(
              () => fail(new Error(`timeout esperando ${what}`)),
              TIMEOUT_MS,
            );
            waiters.push({
              pred,
              resolve: (f) => {
                clearTimeout(timer);
                ok(f);
              },
            });
          }),
      }),
    );
    ws.addEventListener('error', () => reject(new Error(`no se pudo conectar a ${target}`)));
  });
}

const hello = (extra = {}) => ({
  type: 'hello',
  client_msg_id: uuid(),
  protocol_version: 1,
  app_version: 'smoke',
  language: 'es-MX',
  ...extra,
});

const step = (msg) => console.log(`  ✓ ${msg}`);

try {
  console.log(`Smoke test contra ${url}`);
  const a = await open(url);
  a.send(hello());
  const welcome = await a.wait((f) => f.type === 'welcome', 'welcome');
  step(`welcome (sesión ${welcome.session_id.slice(0, 8)}…, entrada: ${welcome.entry})`);

  const create = { type: 'create_incident', client_msg_id: uuid(), channel: 'text_chat' };
  a.send(create);
  await a.wait(
    (f) => f.type === 'ack' && f.client_msg_id === create.client_msg_id,
    'ack de create_incident',
  );
  await a.wait((f) => f.type === 'incident_created', 'incident_created');
  step('incidente creado en el C5 (simulado)');

  const chat = {
    type: 'chat_message',
    client_msg_id: uuid(),
    kind: 'text',
    text: 'Prueba de humo: necesito ayuda',
  };
  a.send(chat);
  await a.wait(
    (f) => f.type === 'ack' && f.client_msg_id === chat.client_msg_id && f.status === 'accepted',
    'ack del chat',
  );
  step('mensaje aceptado (enviado ✓)');
  await a.wait(
    (f) =>
      f.type === 'message_status' &&
      f.client_msg_id === chat.client_msg_id &&
      f.status === 'delivered',
    'delivered',
  );
  step('entregado al C5 (✓✓)');
  await a.wait(
    (f) =>
      f.type === 'message_status' && f.client_msg_id === chat.client_msg_id && f.status === 'read',
    'read',
  );
  step('leído por el operador');
  const reply = await a.wait(
    (f) => f.type === 'chat_message' && f.author === 'operator',
    'respuesta del operador',
  );
  step(`operador respondió: "${reply.text.slice(0, 50)}…"`);

  const seen = Math.max(...a.frames.filter((f) => typeof f.seq === 'number').map((f) => f.seq));
  a.ws.close();

  // Reanudar desde un seq anterior: el BFF debe reenviar lo que falta.
  const b = await open(url);
  const from = Math.max(0, seen - 2);
  b.send(
    hello({
      resume: {
        session_id: welcome.session_id,
        session_token: welcome.session_token,
        last_seq: from,
      },
    }),
  );
  const resumed = await b.wait((f) => f.type === 'welcome', 'welcome de reanudación');
  if (!resumed.resumed) throw new Error('la sesión no se reanudó');
  await b.wait((f) => f.seq === from + 1, `reenvío del seq ${from + 1}`);
  step(`sesión reanudada y recuperada desde seq ${from} (último ${resumed.latest_seq})`);
  b.ws.close();
  console.log('✓ Smoke test OK');
  process.exit(0);
} catch (error) {
  console.error(`✗ Smoke test falló: ${error.message}`);
  process.exit(1);
}
