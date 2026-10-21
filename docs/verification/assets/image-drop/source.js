imports.gi.versions.Gtk = '3.0';
const {Gtk, Gdk, GLib} = imports.gi;
Gtk.init(null);
let win=new Gtk.Window({title:'Synthetic file drag source',default_width:230,default_height:145});
win.move(960,20);
let button=new Gtk.Button({label:'Drag synthetic files'});
button.drag_source_set(Gdk.ModifierType.BUTTON1_MASK,[Gtk.TargetEntry.new('text/uri-list',0,0)],Gdk.DragAction.COPY);
button.connect('drag-begin',()=>print('BEGIN'));
button.connect('drag-end',()=>print('END'));
button.connect('drag-failed',(_,context,result)=>{print('FAILED '+result);return false;});
button.connect('drag-data-get',(_,context,data)=>{
 const [ok,bytes]=GLib.file_get_contents('/tmp/fp-drop-20261022/uris.json');
 if(!ok)throw Error('fixture URI read failed');
 print('DATA');data.set_uris(JSON.parse(imports.byteArray.toString(bytes)));
});
win.connect('destroy',()=>Gtk.main_quit());win.add(button);win.show_all();Gtk.main();
