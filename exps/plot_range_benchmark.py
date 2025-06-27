import os
import argparse
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='Process some CSV files.')
    parser.add_argument('db_name', type=str, help='The name of the tested db')
    parser.add_argument('test_name', type=str, help='The name of the test')
    args = parser.parse_args()

    detail_dir = f'results_{args.db_name}/{args.test_name}'
    summary_file = f'results_{args.db_name}/{args.test_name}_summary.csv'
    summary_plot_file = f'results_{args.db_name}/{args.test_name}_summary.png'
    
    summary_dict = {"entry_count":[], "value_size":[], "range_size": [], 
                  "latency":[], "throughput":[]}
    detail_files = []
    for entry in os.listdir(detail_dir):
        full_path = os.path.join(detail_dir, entry)
        if os.path.isfile(full_path) and entry.endswith('.csv'):
            fname = full_path.split('/')[-1].split('.')[0]
            fname, vl = fname.split('v')
            vl = int(vl)
            acc = int(fname.strip('e'))
            detail_files.append((acc, vl, fname, full_path))
    detail_files = sorted(detail_files)
    for acc, vl, fn, fp in detail_files:
        df = pd.read_csv(fp)
        
        ranges = df['range'].unique()  
        for r in ranges:
            if r != -1:
                data = df[df['range'] == r]
                summary_dict["entry_count"].append(acc)
                summary_dict["value_size"].append(vl)
                summary_dict["range_size"].append(r)
            
                summary_dict["latency"].append(np.mean(data['latency'].to_numpy()))
                summary_dict["throughput"].append(np.mean(data['throughput'].to_numpy()))
    print(summary_dict)    
    summary_df =  pd.DataFrame(summary_dict)
    summary_df.to_csv(summary_file, index=False)