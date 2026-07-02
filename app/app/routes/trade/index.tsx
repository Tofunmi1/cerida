import { redirect } from 'react-router'

export function loader() {
  return redirect('/trade/btc')
}

export default function TradeIndex() {
  return null
}
