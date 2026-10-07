// SPDX-License-Identifier: AGPL-3.0-only
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { getChunks, openSearchResult, type IndexedFileInfo } from "../../../lib/tauri";
import ContentRenderer from "../ContentRenderer";
import PageInfoDrawer from "../page/PageInfoDrawer";
import "./source-access.css";

export default function SourceDocumentPreview({file,onClose,space}:{file:IndexedFileInfo;onClose:()=>void;space?:string}) {
 const {t}=useTranslation();
 const source=['file','webpage'].includes(file.source)?file.source:'memory';
 const query=useQuery({queryKey:['sourceDocument',space ?? null,source,file.source_id],queryFn:()=>space === undefined ? getChunks(source,file.source_id) : getChunks(source,file.source_id,space),retry:false});
 let url:string|null=null;
 // A webpage's source identity is its saved address. Legacy inventories could
 // mix aggregate metadata across source kinds that share an ID.
 try{const parsed=new URL(file.source==='webpage'?file.source_id:(file.url??''));if(['http:','https:'].includes(parsed.protocol)&&!parsed.username&&!parsed.password)url=parsed.href;}catch{/* Not a web address. */}
 const content=[...(query.data??[])].sort((a,b)=>a.chunk_index-b.chunk_index).map(chunk=>chunk.content).join('\n\n');
 return <PageInfoDrawer open docked={space !== undefined} title={file.title} closeLabel={t('sourceAccess.close')} onClose={onClose}>
   {url&&<div className="source-preview-url"><p>{url}</p><button className="page-editor-action" type="button" onClick={()=>void openSearchResult(url!).catch(()=>toast(t('sourceAccess.openFailed')))}>{t('sourceAccess.openOriginal')}</button></div>}
   {query.isPending?<p role="status" className="source-preview-note">{t('sourceAccess.previewLoading')}</p>:query.isError?<div role="alert"><p className="source-preview-note">{t('sourceAccess.previewFailed')}</p><button type="button" className="page-editor-action" onClick={()=>void query.refetch()}>{t('sourceAccess.retry')}</button></div>:content?<div className="source-preview-content"><ContentRenderer content={content} variant="detail" allowImages={false} /></div>:<p className="source-preview-note">{t('sourceAccess.noPreview')}</p>}
 </PageInfoDrawer>;
}
