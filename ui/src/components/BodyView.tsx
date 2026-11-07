import { jsonTokens, prettyBody } from '../format'

export function BodyView({ body, contentType }: { body: string; contentType: string }) {
  const { text, json } = prettyBody(body, contentType)
  if (!text) return <p className="quiet">The response has no body.</p>
  return (
    <pre className="code">
      {json ? jsonTokens(text).map((t, i) => (t.kind === 'plain' ? t.text : <span key={i} className={`t-${t.kind}`}>{t.text}</span>)) : text}
    </pre>
  )
}
