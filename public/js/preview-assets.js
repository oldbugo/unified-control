// Browser-preview storage only; the native app uses its validated AssetStore.
function database(){return new Promise((resolve,reject)=>{const request=indexedDB.open('uc-preview-assets',1);request.onupgradeneeded=()=>request.result.createObjectStore('pictures');request.onsuccess=()=>resolve(request.result);request.onerror=()=>reject(new Error('Picture storage unavailable'));});}
export async function savePreviewPicture(file){
  if(!['image/png','image/jpeg'].includes(file.type) || file.size>4*1024*1024)throw new Error('Choose a PNG or JPEG of at most 4 MiB');
  const image=await createImageBitmap(file);const valid=image.width<=4096&&image.height<=4096&&image.width*image.height<=16_000_000;image.close();
  if(!valid)throw new Error('The picture is too large');
  const id=`asset-${crypto.randomUUID()}`,db=await database();
  try{await new Promise((resolve,reject)=>{const tx=db.transaction('pictures','readwrite');tx.objectStore('pictures').add(file,id);tx.oncomplete=resolve;tx.onerror=()=>reject(new Error('The picture could not be stored'));});}finally{db.close();}
  return id;
}
export async function previewPicture(id){const db=await database();try{return await new Promise((resolve,reject)=>{const request=db.transaction('pictures').objectStore('pictures').get(id);request.onsuccess=()=>resolve(request.result);request.onerror=reject;});}finally{db.close();}}
