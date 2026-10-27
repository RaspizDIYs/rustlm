import type {NetworkReport,Probe} from "./network";
import {critical} from "./repair-policy";
export function infrastructureDomain(domain:string){return [".pacloudflare.com",".akamai.net",".akamaiedge.net",".edgekey.net",".edgesuite.net",".cdn.cloudflare.net"].some(s=>domain.endsWith(s));}
export function noIpv6Route(probe:Probe){return probe.ip.includes(":") && !probe.tcp && /10051|network is unreachable|сети недоступ|отключенной сети/i.test(probe.error??"");}
export function networkSummary(report:NetworkReport){
 const services=new Map<string,Probe[]>();
 const infrastructure=new Set<string>();
 for(const p of report.probes){
  if(infrastructureDomain(p.domain) || (report.service_domains?.length && !report.service_domains.includes(p.domain))){infrastructure.add(p.domain);continue;}
  if(!critical(p.domain,p.port,report.context.region))continue;
  const key=`${p.domain}:${p.port}${p.request_path}`;services.set(key,[...(services.get(key)??[]),p]);
 }
 const groups=[...services].map(([name,probes])=>{
  const ipv4=probes.filter(p=>!p.ip.includes(":"));const selected=ipv4.length?ipv4:probes;
  const worked=selected.filter(p=>p.tls);
  const status=worked.some(p=>p.classification==="http_success"||p.classification==="tls_success")?"responding":worked.length?"restricted":selected.every(noIpv6Route)?"unavailable_ipv6":"needs_check";
  return {name,status};
 });
 const unresolved=[...new Set(report.dns.filter(d=>!infrastructureDomain(d.domain)).map(d=>d.domain))].filter(domain=>!report.dns.some(d=>d.domain===domain&&d.addresses.length));
 const brokenSystemDns=[...new Set(report.dns.filter(d=>d.resolver==="system"&&!d.addresses.length&&report.dns.some(other=>other.domain===d.domain&&other.resolver!=="system"&&other.addresses.length)).map(d=>d.domain))];
 return {groups,infrastructure:infrastructure.size,ipv6Unavailable:report.probes.filter(noIpv6Route).length,unresolved,brokenSystemDns,responding:groups.filter(g=>g.status==="responding").length,restricted:groups.filter(g=>g.status==="restricted").length,needsCheck:groups.filter(g=>g.status==="needs_check")};
}
