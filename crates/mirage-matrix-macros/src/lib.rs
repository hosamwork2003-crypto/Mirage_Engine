use proc_macro::TokenStream;
use quote::{quote, format_ident};
use syn::{parse_macro_input, DeriveInput, Data, Fields};

#[proc_macro_derive(NeuralCluster, attributes(synapse))]
pub fn neural_cluster_derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    let mut field_names = Vec::new();
    let mut field_types = Vec::new(); 
    let mut setter_names = Vec::new(); 
    
    if let Data::Struct(data) = input.data {
        if let Fields::Named(fields) = data.fields {
            for field in fields.named {
                if let Some(ident) = &field.ident {
                    field_names.push(ident.clone());
                    field_types.push(field.ty.clone());
                    setter_names.push(format_ident!("set_{}", ident));
                }
            }
        }
    }

    let expanded = quote! {
        impl #name {
            /// ربط الكيان بشبكة الـ Matrix وتوليد مقابض (Handles) سريعة
            pub fn wire_to_matrix(
                &self, 
                matrix: &mut NeuralMatrix, 
                directory: &mut mirage_core::pool::RuntimeDirectory
            ) -> Vec<mirage_core::pool::Handle> {
                println!("🔌 Wiring Cluster [{}] into the Neural Matrix...", stringify!(#name));
                let mut handles = std::vec::Vec::new();
                
                #(
                    // تسجيل حقل وهمي مؤقتاً في الدليل للحصول على Handle سريع
                    let dummy_uuid = mirage_core::oasis::uuid::MirageUuid::new();
                    let handle = directory.register_entity(dummy_uuid, 0, 0, 0);
                    handles.push(handle);
                    
                    println!("   🟢 Wired Node [{}]: Handle Index {}", stringify!(#field_names), handle.index());
                )*
                
                handles 
            }

            #(
                pub fn #setter_names(
                    &mut self, 
                    new_value: #field_types, 
                    matrix: &NeuralMatrix, 
                    directory: &mirage_core::pool::RuntimeDirectory,
                    handle: mirage_core::pool::Handle
                ) {
                    self.#field_names = new_value;
                    println!("💉 Mutation on [{}]: Value changed. Injecting pulse...", stringify!(#field_names));
                    // إرسال النبضة باستخدام Handle بدلاً من NodeIndex
                    matrix.trace_impact(handle, directory);
                }
            )*
        }
    };

    expanded.into()
}