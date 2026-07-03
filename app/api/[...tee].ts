// Proxy TEE API calls from the HTTPS Vercel deployment to the GCP TEE server.
// Avoids CORS and mixed-content issues — the proxy runs server-side on Vercel.
//
// Frontend calls: POST /api/tee/init          → forwards to http://TEE:9721/init
//                 POST /api/tee/note-proof     → forwards to http://TEE:9721/note-proof

const TEE = 'http://35.255.76.255:9721'

export const config = { runtime: 'edge' }

export async function POST(request: Request) {
  const url = new URL(request.url)
  const path = url.pathname.replace('/api/tee', '')

  try {
    const body = await request.text()
    const resp = await fetch(`${TEE}${path}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body,
    })

    const text = await resp.text()
    return new Response(text, {
      status: resp.status,
      headers: { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' },
    })
  } catch (e: any) {
    return new Response(JSON.stringify({ ok: false, error: e.message }), {
      status: 502,
      headers: { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' },
    })
  }
}

export async function GET(request: Request) {
  const url = new URL(request.url)
  const path = url.pathname.replace('/api/tee', '')

  try {
    const resp = await fetch(`${TEE}${path}`)
    const text = await resp.text()
    return new Response(text, {
      status: resp.status,
      headers: { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' },
    })
  } catch (e: any) {
    return new Response(JSON.stringify({ ok: false, error: e.message }), {
      status: 502,
      headers: { 'Content-Type': 'application/json', 'Access-Control-Allow-Origin': '*' },
    })
  }
}
