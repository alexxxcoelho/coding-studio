export type DiffPart={before:string;after:string;changed:boolean;key:string}
export function textDiff(before:string,after:string,prefix=''):DiffPart[]{
 if(before===after)return [{before,after,changed:false,key:prefix}]
 const a=before.match(/\S+\s*|\s+/gu)??[],b=after.match(/\S+\s*|\s+/gu)??[]
 if(a.length*b.length>1000000)return [{before,after,changed:true,key:JSON.stringify([prefix,before,after])}]
 const width=b.length+1,table=new Uint16Array((a.length+1)*width)
 for(let i=a.length-1;i>=0;i--)for(let j=b.length-1;j>=0;j--)table[i*width+j]=a[i]===b[j]?table[(i+1)*width+j+1]+1:Math.max(table[(i+1)*width+j],table[i*width+j+1])
 const parts:DiffPart[]=[];let i=0,j=0,old='',fresh=''
 const flush=()=>{if(old||fresh){parts.push({before:old,after:fresh,changed:true,key:JSON.stringify([prefix,parts.length,old,fresh])});old='';fresh=''}}
 while(i<a.length||j<b.length){if(i<a.length&&j<b.length&&a[i]===b[j]){flush();parts.push({before:a[i],after:b[j],changed:false,key:''});i++;j++}else if(i<a.length&&(j===b.length||table[(i+1)*width+j]>=table[i*width+j+1]))old+=a[i++];else fresh+=b[j++]}
 flush();return parts
}
export const reviewedText=(parts:DiffPart[],decisions:Record<string,'accept'|'reject'>)=>parts.map(p=>!p.changed||decisions[p.key]==='reject'?p.before:p.after).join('')
