import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter } from 'k6/metrics';

// Contador de falhas por status code, para diagnosticar rejeições (429 = throttling)
const failuresByStatus = new Counter('failures_by_status');

// Configuração do cenário de carga: 100 usuários simultâneos por 10 minutos, sem rampa
export const options = {
  vus: 100,
  duration: '10m',
  thresholds: {
    // Falha o teste se 95% das requisições não forem respondidas em menos de 500ms
    http_req_duration: ['p(95)<500'],
    // Falha o teste se a taxa de erro for maior que 1%
    http_req_failed: ['rate<0.01'],
    // Sempre passam: existem só para o sumário exibir a contagem de cada status de falha
    'failures_by_status{status:429}': ['count>=0'],
    'failures_by_status{status:500}': ['count>=0'],
    'failures_by_status{status:502}': ['count>=0'],
    'failures_by_status{status:503}': ['count>=0'],
  },
};

// URLs dos endpoints por runtime
const URLS = {
  rust: 'https://z9ytmykvfe.execute-api.sa-east-1.amazonaws.com/',
  java: 'https://qcv9uentyk.execute-api.sa-east-1.amazonaws.com/',
};

// Define o alvo via variável de ambiente: k6 run -e TARGET=rust|java (padrão: rust)
const target = (__ENV.TARGET || 'rust').toLowerCase();
if (!URLS[target]) {
  throw new Error(`TARGET inválido: "${target}". Use "rust" ou "java".`);
}

export default function () {
  const url = URLS[target];

  // Geração de ID dinâmico para evitar colisões caso o backend valide duplicidade de pedidos
  const dynamicOrderId = Math.floor(Math.random() * 1000000000).toString();

  const payload = JSON.stringify({
    "order_id": dynamicOrderId, // Substitua por "123456" se preferir o ID estático
    "customer_name": "Fulano de Tal",
    "total": 10000
  });

  const params = {
    headers: {
      'Content-Type': 'application/json',
      'X-Route-Key': 'order-created',
    },
  };

  // Executa o POST HTTP
  const res = http.post(url, payload, params);

  // Valida a resposta da API (202 = pedido aceito para processamento assíncrono)
  const ok = check(res, {
    'status sucesso (200, 201 ou 202)': (r) => r.status === 200 || r.status === 201 || r.status === 202,
  });

  if (!ok) {
    failuresByStatus.add(1, { status: String(res.status) });
    // Loga um exemplo de falha (só o VU 1, nas primeiras iterações, para não inundar o console)
    if (__VU === 1 && __ITER < 3) {
      console.error(`Falha exemplo: status=${res.status} body=${res.body}`);
    }
  }

  // Pausa média de ~1s com jitter para espalhar as requisições dos 100 VUs
  // (sleep fixo sincroniza os VUs em rajadas de 100 simultâneas, estourando o
  // limite de concorrência da conta — 10 — e gerando 503 no API Gateway)
  sleep(0.5 + Math.random());
}